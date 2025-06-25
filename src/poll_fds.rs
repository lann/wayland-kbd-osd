// This module will contain the FdPoller struct and related types for handling
// polling of file descriptors.
// Initially, it will support Wayland and libinput file descriptors.

use libc;
use std::os::unix::io::AsRawFd; // Import AsRawFd trait
use wayland_client::Connection;
use input::Libinput; // Assuming this is the correct type for the libinput context

const WAYLAND_FD_INDEX: usize = 0;
// LIBINPUT_FD_INDEX will be 1 if libinput_fd is Some.
// Note: If we add more FDs, this simple indexing might need to be more robust.
const LIBINPUT_FD_INDEX: usize = 1;

/// Error type for FdPoller creation, specifically if Wayland FD cannot be obtained.
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

        fds.push(libc::pollfd {
            fd: wayland_fd,
            events: libc::POLLIN,
            revents: 0,
        });

        let libinput_raw_fd_opt = libinput_ctx.map(|ctx| ctx.as_raw_fd());

        let has_libinput = if let Some(fd) = libinput_raw_fd_opt {
            fds.push(libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
            true
        } else {
            false
        };

        Ok(FdPoller { fds, has_libinput })
    }

    pub fn poll(&mut self, timeout_ms: i32) -> Result<Vec<PollEvent>, PollError> {
        // Reset revents before each poll call
        for pfd in self.fds.iter_mut() {
            pfd.revents = 0;
        }

        let ret = unsafe {
            libc::poll(self.fds.as_mut_ptr(), self.fds.len() as libc::nfds_t, timeout_ms)
        };

        if ret < 0 {
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::EINTR {
                return Err(PollError::Interrupted);
            }
            return Err(PollError::Other(errno));
        }

        if ret == 0 {
            return Ok(vec![PollEvent::Timeout]);
        }

        let mut events = Vec::with_capacity(ret as usize);

        // Check Wayland FD
        let wayland_pfd = &self.fds[WAYLAND_FD_INDEX];
        if (wayland_pfd.revents & libc::POLLERR) != 0 || (wayland_pfd.revents & libc::POLLHUP) != 0 {
            events.push(PollEvent::WaylandError);
        } else if (wayland_pfd.revents & libc::POLLIN) != 0 {
            events.push(PollEvent::WaylandReady);
        }

        // Check Libinput FD if it exists
        if self.has_libinput && self.fds.len() > LIBINPUT_FD_INDEX { // Ensure index is valid
            let libinput_pfd = &self.fds[LIBINPUT_FD_INDEX];
            if (libinput_pfd.revents & libc::POLLERR) != 0 || (libinput_pfd.revents & libc::POLLHUP) != 0 {
                events.push(PollEvent::LibinputError);
            } else if (libinput_pfd.revents & libc::POLLIN) != 0 {
                events.push(PollEvent::LibinputReady);
            }
        }

        // If poll returned > 0 but we didn't identify specific events (e.g. only POLLNVAL),
        // it's an unexpected state. For now, we return what we have.
        // If events is empty here and ret > 0, it might indicate an issue or an unhandled revent type.
        if events.is_empty() && ret > 0 {
            // This case should ideally not be reached if FDs are valid and events are POLLIN/ERR/HUP.
            // Consider logging if this happens.
            // For now, if poll said there are events, but we didn't categorize them,
            // it's better to return an empty vec than a Timeout, as it wasn't a timeout.
        }


        Ok(events)
    }
}
