// This module contains the FdPoller struct and related types for handling
// polling of file descriptors using the `poll` syscall.
// It supports Wayland and libinput file descriptors.

use libc;
use std::os::unix::io::AsRawFd;
use wayland_client::Connection;
use input::Libinput;

// Indices for accessing file descriptors in the `fds` vector.
const WAYLAND_FD_INDEX: usize = 0;
const LIBINPUT_FD_INDEX: usize = 1;
// NOTE: If more file descriptor types are added, this simple indexing approach
// will need to be made more robust (e.g., using an enum to map FD types to indices
// or a more dynamic structure).

/// Error type for FdPoller creation.
#[derive(Debug)]
pub enum FdPollerCreationError {
    WaylandConnection(wayland_client::backend::WaylandError),
}

impl std::fmt::Display for FdPollerCreationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FdPollerCreationError::WaylandConnection(e) => write!(f, "Wayland connection error: {}", e),
        }
    }
}

impl std::error::Error for FdPollerCreationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FdPollerCreationError::WaylandConnection(e) => Some(e),
        }
    }
}


pub struct FdPoller {
    fds: Vec<libc::pollfd>,
    has_libinput: bool,
    // Keep the read guard alive for the lifetime of FdPoller if needed,
    // though for just getting the FD, it's not strictly necessary to store it.
    // _wayland_read_guard: Option<wayland_client::backend::ReadEventsGuard<'static>>, // This might be tricky with lifetimes
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PollEvent {
    WaylandReady,
    LibinputReady,
    WaylandError, // POLLERR or POLLHUP on Wayland FD
    LibinputError, // POLLERR or POLLHUP on Libinput FD
    Timeout,
    // Errors from the poll call itself are returned as Result::Err
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PollError {
    Interrupted, // Corresponds to EINTR
    Other(i32),  // Other unhandled errno from poll
}

impl FdPoller {
    pub fn new(
        wayland_conn: &Connection,
        libinput_ctx: Option<&Libinput>,
    ) -> Result<Self, FdPollerCreationError> {
        let mut fds = Vec::with_capacity(2);

        // Obtain Wayland FD. prepare_read() returns a guard.
        // The FD is valid as long as the Connection is valid.
        // We don't need to keep the guard itself if we're only getting the FD.
        let wayland_fd = wayland_conn
            .prepare_read()
            .map_err(FdPollerCreationError::WaylandConnection)?
            .connection_fd()
            .as_raw_fd();

        // Setup pollfd for Wayland:
        // - fd: The Wayland connection file descriptor.
        // - events: Flags specifying the events to monitor.
        //   - libc::POLLIN: Monitor for readable data. This is the primary event for Wayland.
        // - revents: Output field, filled by poll() with events that occurred (e.g., POLLIN, POLLERR).
        fds.push(libc::pollfd {
            fd: wayland_fd,
            events: libc::POLLIN,
            revents: 0, // Must be initialized to 0 before calling poll()
        });

        let libinput_raw_fd_opt = libinput_ctx.map(|ctx| ctx.as_raw_fd());

        let has_libinput = if let Some(libinput_fd) = libinput_raw_fd_opt {
            // Setup pollfd for libinput (if context exists):
            // - fd: The libinput file descriptor.
            // - events: libc::POLLIN to monitor for readable input events.
            fds.push(libc::pollfd {
                fd: libinput_fd,
                events: libc::POLLIN,
                revents: 0, // Must be initialized to 0
            });
            true
        } else {
            false
        };

        Ok(FdPoller { fds, has_libinput })
    }

    pub fn poll(&mut self, timeout_ms: i32) -> Result<Vec<PollEvent>, PollError> {
        // Reset revents before each poll call, as poll() only sets bits for new events.
        for pfd in self.fds.iter_mut() {
            pfd.revents = 0;
        }

        // Call the poll system call.
        // `fds.as_mut_ptr()`: Pointer to the array of pollfd structs.
        // `fds.len() as libc::nfds_t`: Number of items in the array.
        // `timeout_ms`: Timeout in milliseconds. -1 for infinite, 0 for immediate return.
        let num_events = unsafe {
            libc::poll(self.fds.as_mut_ptr(), self.fds.len() as libc::nfds_t, timeout_ms)
        };

        // Handle poll() return values.
        if num_events < 0 {
            // An error occurred during poll().
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::EINTR {
                // Interrupted by a signal, this is usually not a fatal error.
                log::trace!("poll() interrupted by signal (EINTR)");
                return Err(PollError::Interrupted);
            } else {
                // Other, potentially more serious, errors.
                log::error!("poll() failed with errno: {}", errno);
                return Err(PollError::Other(errno));
            }
        }

        if num_events == 0 {
            // Timeout occurred, no file descriptors are ready.
            return Ok(vec![PollEvent::Timeout]);
        }

        // If num_events > 0, one or more file descriptors have events.
        let mut events_triggered = Vec::with_capacity(num_events as usize);

        // Check Wayland FD events.
        // `revents` contains the events that actually occurred for this FD.
        // - libc::POLLIN: Data is available to read.
        // - libc::POLLERR: An error occurred on the FD.
        // - libc::POLLHUP: Hang up occurred on the FD (e.g., connection closed).
        // - libc::POLLNVAL: Invalid request (e.g., fd not open). Should not happen with valid FDs.
        let wayland_pfd = &self.fds[WAYLAND_FD_INDEX];
        if (wayland_pfd.revents & libc::POLLERR) != 0 {
            log::warn!("POLLERR on Wayland FD");
            events_triggered.push(PollEvent::WaylandError);
        } else if (wayland_pfd.revents & libc::POLLHUP) != 0 {
            log::warn!("POLLHUP on Wayland FD");
            events_triggered.push(PollEvent::WaylandError);
        } else if (wayland_pfd.revents & libc::POLLNVAL) != 0 {
            log::error!("POLLNVAL on Wayland FD - this indicates a serious issue!");
            events_triggered.push(PollEvent::WaylandError); // Treat as error
        } else if (wayland_pfd.revents & libc::POLLIN) != 0 {
            events_triggered.push(PollEvent::WaylandReady);
        }

        // Check Libinput FD events, if it exists.
        if self.has_libinput && self.fds.len() > LIBINPUT_FD_INDEX { // Ensure index is valid
            let libinput_pfd = &self.fds[LIBINPUT_FD_INDEX];
            if (libinput_pfd.revents & libc::POLLERR) != 0 {
                log::warn!("POLLERR on Libinput FD");
                events_triggered.push(PollEvent::LibinputError);
            } else if (libinput_pfd.revents & libc::POLLHUP) != 0 {
                log::warn!("POLLHUP on Libinput FD");
                events_triggered.push(PollEvent::LibinputError);
            } else if (libinput_pfd.revents & libc::POLLNVAL) != 0 {
                log::error!("POLLNVAL on Libinput FD - this indicates a serious issue!");
                events_triggered.push(PollEvent::LibinputError); // Treat as error
            } else if (libinput_pfd.revents & libc::POLLIN) != 0 {
                events_triggered.push(PollEvent::LibinputReady);
            }
        }

        if events_triggered.is_empty() && num_events > 0 {
            // This case means poll() reported events, but none of the specific conditions
            // (POLLIN, POLLERR, POLLHUP, POLLNVAL) were matched for the known FDs.
            // This is unexpected if FDs are correctly set up.
            log::warn!(
                "poll() reported {} events, but no specific POLLIN/ERR/HUP/NVAL was handled for known FDs. Wayland revents: {:#X}, Libinput revents: {:#X}",
                num_events,
                wayland_pfd.revents,
                if self.has_libinput && self.fds.len() > LIBINPUT_FD_INDEX { self.fds[LIBINPUT_FD_INDEX].revents } else { 0 }
            );
        }

        Ok(events_triggered)
    }
}
