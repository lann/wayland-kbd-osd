// Input event handling

use input::event::keyboard::{KeyState, KeyboardEvent, KeyboardEventTrait};
use input::event::Event as LibinputEvent;
use libc::{O_NONBLOCK, O_RDWR};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, OwnedFd}; // Added AsRawFd
use std::path::Path;

// Assuming AppState is defined in wayland.rs and passed here
use crate::wayland::AppState;

pub struct MyLibinputInterface;

impl input::LibinputInterface for MyLibinputInterface {
    fn open_restricted(&mut self, path: &Path, _flags: i32) -> Result<OwnedFd, i32> {
        // flags are ignored because O_RDWR | O_NONBLOCK are always used.
        log::debug!("Attempting to open input device: {:?}", path);
        OpenOptions::new()
            .custom_flags(O_RDWR | O_NONBLOCK) // Explicitly use these flags
            .read(true) // Required by libinput
            .write(true) // Required by libinput for some operations like LED changes (though not used by this app)
            .open(path)
            .map(|file| file.into())
            .map_err(|e| {
                let errno = e.raw_os_error().unwrap_or(libc::EIO);
                match errno {
                    libc::EPERM => log::error!(
                        "Permission denied when opening {:?}. Check user permissions (e.g., 'input' group). Error: {}",
                        path, e
                    ),
                    libc::ENOENT => log::error!(
                        "Device {:?} not found. It might have been unplugged. Error: {}",
                        path, e
                    ),
                    libc::EACCES => log::error!( // EACCES can also mean permission issues
                        "Access denied when opening {:?}. Similar to EPERM, check permissions. Error: {}",
                        path, e
                    ),
                    _ => log::error!(
                        "Failed to open input device {:?} with flags O_RDWR | O_NONBLOCK. Error: {} (errno: {})",
                        path, e, errno
                    ),
                }
                errno // Return the original errno
            })
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        // OwnedFd handles closing the file descriptor when it's dropped.
        // We can log the action if desired.
        log::debug!("Closing input device (FD: {}) via OwnedFd drop.", fd.as_raw_fd());
        drop(fd);
    }
}

pub fn handle_libinput_events(app_state: &mut AppState) {
    if let Some(ref mut context) = app_state.input_context {
        if let Err(e) = context.dispatch() {
            // An error from dispatch() typically means libinput is no longer usable.
            // This might happen if the underlying udev resources are gone.
            log::error!("Libinput dispatch error: {}. Input monitoring may stop.", e);
            // Consider setting a flag to stop trying to use libinput or reinitialize.
            // For now, we'll log and continue; FdPoller might detect FD errors later.
            return;
        }

        for event in context.by_ref() {
            if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = event {
                let key_code = key_event.key(); // This is the raw scancode from libinput
                let key_state = key_event.key_state();
                let pressed = key_state == KeyState::Pressed;

                // Attempt to find the key name from our config for logging
                // This is a linear search, might be slow if there are many keys.
                // For frequent logging, consider a reverse map if performance becomes an issue.
                let key_name_for_log: String = app_state
                    .config
                    .key
                    .iter()
                    .find(|k| k.keycode == key_code)
                    .map(|k| k.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());

                if let Some(current_state) = app_state.key_states.get_mut(&key_code) {
                    if *current_state != pressed {
                        *current_state = pressed;
                        app_state.needs_redraw = true;
                        log::debug!(
                            "Key Event: Code {}, Name '{}', State: {:?} -> {}",
                            key_code,
                            key_name_for_log,
                            key_state,
                            if pressed { "Pressed" } else { "Released" }
                        );
                    }
                } else {
                    // This case should ideally not happen if key_states is populated correctly
                    // from all keys defined in the config.
                    log::warn!(
                        "Key Event for unmonitored key: Code {}, Name '{}', State: {:?}",
                        key_code,
                        key_name_for_log,
                        key_state
                    );
                }
            }
        }
    }
}
