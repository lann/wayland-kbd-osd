// Input event handling

use input::event::keyboard::{KeyState, KeyboardEvent, KeyboardEventTrait};
use input::event::Event as LibinputEvent;
use libc::{O_NONBLOCK, O_RDWR};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::OwnedFd;
use std::path::Path;

// Assuming AppState is defined in wayland.rs and passed here
use crate::wayland::AppState;

pub struct MyLibinputInterface;

impl input::LibinputInterface for MyLibinputInterface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        log::debug!("Opening path: {:?}, flags: {}", path, flags);
        OpenOptions::new()
            .custom_flags(O_RDWR | O_NONBLOCK) // Use the correct flags
            .read(true)
            .write(true)
            .open(path)
            .map(|file| file.into())
            .map_err(|e| {
                log::error!("Failed to open path {:?}: {}", path, e);
                e.raw_os_error().unwrap_or(libc::EIO)
            })
    }
    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(fd);
        log::debug!("Closed device via OwnedFd drop");
    }
}

pub fn handle_libinput_events(app_state: &mut AppState) {
    if let Some(ref mut context) = app_state.input_context {
        if context.dispatch().is_err() {
            log::error!("Libinput dispatch error");
        }
        for event in context.by_ref() {
            if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = event {
                let key_code = key_event.key();
                let pressed = key_event.key_state() == KeyState::Pressed;
                if let Some(current_state) = app_state.key_states.get_mut(&key_code) {
                    if *current_state != pressed {
                        *current_state = pressed;
                        app_state.needs_redraw = true;
                        log::debug!("Key {} state changed to: {}", key_code, pressed);
                    }
                }
            }
        }
    }
}
