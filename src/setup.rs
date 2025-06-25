// src/setup.rs

use crate::config::{self, AppConfig};
use crate::event::MyLibinputInterface;
use crate::poll_fds::{FdPoller, FdPollerCreationError};
use crate::wayland::AppState;

use wayland_client::protocol::wl_output;
use wayland_client::{Connection, EventQueue, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1,
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity},
};

use std::process;

pub fn initialize_wayland_connection() -> Connection {
    Connection::connect_to_env().unwrap_or_else(|e| {
        log::error!("Failed to connect to Wayland display: {}", e);
        eprintln!("Failed to connect to Wayland display. Is a Wayland compositor running?");
        process::exit(1);
    })
}

pub fn initialize_globals_and_outputs(
    conn: &Connection,
    event_queue: &mut EventQueue<AppState>,
    app_state: &mut AppState,
) {
    log::trace!("Dispatching initial events to bind globals and get initial output info...");
    for _ in 0..3 {
        if event_queue.dispatch_pending(app_state).is_err() {
            log::error!("Error dispatching events during initial setup.");
            break;
        }
        if conn.flush().is_err() {
            log::error!("Error flushing connection during initial setup.");
            break;
        }
    }
    if event_queue.roundtrip(app_state).is_err() {
        log::error!("Error during final initial roundtrip for global binding.");
    }

    if app_state.compositor.is_none()
        || app_state.shm.is_none()
        || app_state.xdg_wm_base.is_none()
    {
        log::error!(
            "Failed to bind essential Wayland globals (wl_compositor, wl_shm, xdg_wm_base)."
        );
        eprintln!("Could not bind essential Wayland globals. This usually means the Wayland compositor is missing support or encountered an issue.");
        process::exit(1);
    }
    log::info!(
        "Essential Wayland globals bound. Number of outputs found: {}",
        app_state.outputs.len()
    );
    for (idx, (name, _, _, info)) in app_state.outputs.iter().enumerate() {
        log::debug!(
            "  Output [{}], ID: {}, Name: {:?}, Desc: {:?}, Logical Dims: {}x{}",
            idx,
            name,
            info.name.as_deref().unwrap_or("N/A"),
            info.description.as_deref().unwrap_or("N/A"),
            info.logical_width,
            info.logical_height
        );
    }
}

pub fn initialize_libinput_context(app_state: &mut AppState) {
    let interface = MyLibinputInterface;
    let mut libinput_context = input::Libinput::new_with_udev(interface);
    match libinput_context.udev_assign_seat("seat0") {
        Ok(_) => {
            log::info!("Successfully assigned seat0 to libinput context.");
            app_state.input_context = Some(libinput_context);
        }
        Err(e) => {
            log::warn!("Failed to assign seat0 to libinput context: {:?}. Input monitoring will be disabled.", e);
            log::warn!("This may be due to permissions issues. Ensure the user is in the 'input' group or has direct access to /dev/input/event* devices.");
        }
    }
}

pub fn create_wayland_surface(app_state: &mut AppState, qh: &QueueHandle<AppState>) {
    let surface = app_state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(qh, ());
    app_state.surface = Some(surface);
}

pub fn setup_overlay_mode(app_state: &mut AppState, qh: &QueueHandle<AppState>) {
    log::info!("Overlay mode active (default). Attempting to use wlr-layer-shell.");
    if let Some(layer_shell) = app_state.layer_shell.as_ref() {
        let surface = app_state.surface.as_ref().expect("Surface should exist for overlay mode");
        let mut selected_wl_output_proxy: Option<&wl_output::WlOutput> = None;

        if let Some(target_screen_specifier) = app_state.target_output_identifier.as_ref() {
            log::info!(
                "Attempting to find screen specified in config as: '{}'",
                target_screen_specifier
            );
            if let Ok(target_idx) = target_screen_specifier.parse::<usize>() {
                if let Some((output_registry_name, wl_output_proxy_ref, _, info)) =
                    app_state.outputs.get(target_idx)
                {
                    selected_wl_output_proxy = Some(wl_output_proxy_ref);
                    app_state.identified_target_wl_output_name = Some(*output_registry_name);
                    log::info!(
                        "Selected screen by index {}: ID {}, Name: {:?}",
                        target_idx,
                        output_registry_name,
                        info.name.as_deref().unwrap_or("N/A")
                    );
                } else {
                    log::warn!("Screen index {} from config is out of bounds ({} outputs available). Compositor will choose output.", target_idx, app_state.outputs.len());
                }
            } else {
                let mut matched_by_name = false;
                for (output_registry_name, wl_output_proxy_ref, _, info) in &app_state.outputs {
                    if info.name.as_deref() == Some(target_screen_specifier)
                        || info.description.as_deref() == Some(target_screen_specifier)
                    {
                        selected_wl_output_proxy = Some(wl_output_proxy_ref);
                        app_state.identified_target_wl_output_name = Some(*output_registry_name);
                        matched_by_name = true;
                        log::info!(
                            "Selected screen by name/description '{}': ID {}, Name: {:?}",
                            target_screen_specifier,
                            output_registry_name,
                            info.name.as_deref().unwrap_or("N/A")
                        );
                        break;
                    }
                }
                if !matched_by_name {
                    log::warn!("Screen specifier '{}' from config not found by name or description. Compositor will choose output.", target_screen_specifier);
                    log::debug!("Available outputs for matching by name/description:");
                    for (idx, (id, _, _, i)) in app_state.outputs.iter().enumerate() {
                        log::debug!(
                            "  [{}]: ID: {}, Name: {:?}, Description: {:?}",
                            idx,
                            id,
                            i.name.as_deref().unwrap_or("N/A"),
                            i.description.as_deref().unwrap_or("N/A")
                        );
                    }
                }
            }
        } else {
            log::info!("No specific screen configured (overlay.screen is None). Compositor will choose output.");
        }

        let layer_surface_obj = layer_shell.get_layer_surface(
            surface,
            selected_wl_output_proxy,
                zwlr_layer_shell_v1::Layer::Overlay, // Use the module name for Layer enum
            "wayland-kbd-osd".to_string(),
            qh,
            (),
        );

        let base_anchor = match app_state.config.overlay.position {
            config::OverlayPosition::Top | config::OverlayPosition::TopCenter => {
                    Anchor::Top | Anchor::Left | Anchor::Right
            }
            config::OverlayPosition::Bottom | config::OverlayPosition::BottomCenter => {
                    Anchor::Bottom | Anchor::Left | Anchor::Right
            }
                config::OverlayPosition::Left => Anchor::Left,
                config::OverlayPosition::Right => Anchor::Right,
            config::OverlayPosition::Center => {
                    Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right
            }
                config::OverlayPosition::TopLeft => Anchor::Top | Anchor::Left,
                config::OverlayPosition::TopRight => Anchor::Top | Anchor::Right,
                config::OverlayPosition::BottomLeft => Anchor::Bottom | Anchor::Left,
                config::OverlayPosition::BottomRight => Anchor::Bottom | Anchor::Right,
                config::OverlayPosition::CenterLeft => Anchor::Left,
                config::OverlayPosition::CenterRight => Anchor::Right,
        };

        log::info!("Setting anchor to: {:?}", base_anchor);
        layer_surface_obj.set_anchor(base_anchor);

        let margins = &app_state.config.overlay;
        log::info!(
            "Setting margins: T={}, R={}, B={}, L={}",
            margins.margin_top,
            margins.margin_right,
            margins.margin_bottom,
            margins.margin_left
        );
        layer_surface_obj.set_margin(
            margins.margin_top,
            margins.margin_right,
            margins.margin_bottom,
            margins.margin_left,
        );

            layer_surface_obj.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface_obj.set_exclusive_zone(0);

        log::info!("Setting initial layer surface size to (1,1). Actual size will be configured once screen dimensions are known.");
        layer_surface_obj.set_size(1, 1);

        app_state.layer_surface = Some(layer_surface_obj);
        log::info!("Created and configured layer surface for overlay mode.");
    } else {
        log::error!("Overlay mode active, but zwlr_layer_shell_v1 is not available from the compositor. Falling back to XDG window mode.");
        app_state.is_window_mode = true; // Fallback
        setup_window_mode(app_state, qh); // Call window mode setup
    }
}

pub fn setup_window_mode(app_state: &mut AppState, qh: &QueueHandle<AppState>) {
    log::info!("Window mode active (XDG shell).");
    let surface = app_state.surface.as_ref().expect("Surface should exist for window mode");
    let xdg_surface_proxy = app_state
        .xdg_wm_base
        .as_ref()
        .unwrap()
        .get_xdg_surface(surface, qh, ());
    let toplevel = xdg_surface_proxy.get_toplevel(qh, ());
    let title = if app_state.is_window_mode { // True if called directly or as fallback
        "Wayland Keyboard OSD"
    } else { // Should not happen if logic is correct, but as safety
        "Wayland Keyboard OSD (Fallback Window)"
    };
    toplevel.set_title(title.to_string());
}


pub fn finalize_surface_setup(
    event_queue: &mut EventQueue<AppState>,
    app_state: &mut AppState,
    qh: &QueueHandle<AppState>,
) {
    app_state.surface.as_ref().unwrap().commit();

    log::info!("Initial surface commit done. Dispatching events to catch initial configure...");
    if event_queue.roundtrip(app_state).is_err() {
        log::error!("Error during roundtrip after surface commit (waiting for initial configure).");
    }

    if !app_state.is_window_mode && app_state.layer_shell.is_some() {
        log::info!("Initial layer surface setup complete. Attempting to configure size based on currently known screen dimensions...");
        app_state.attempt_configure_layer_surface_size();

        if !app_state.initial_surface_size_set {
            log::warn!("Initial attempt to set layer surface size deferred as screen dimensions are not yet known or not valid for the target output. Will retry upon receiving output events.");
        }
    }

    log::info!("Initial setup phase complete. Wayland window should be configured or awaiting configuration. Waiting for events...");

    if app_state.needs_redraw && app_state.frame_callback.is_none() {
        if let Some(surface) = app_state.surface.as_ref() {
            log::debug!("Main setup: needs_redraw is true and no frame_callback pending, requesting initial frame callback.");
            let callback = surface.frame(qh, ());
            app_state.frame_callback = Some(callback);
        } else {
            log::warn!("Main setup: needs_redraw is true, but surface is None. Cannot request initial frame callback.");
        }
    }
}

pub fn initialize_fd_poller(
    conn: &Connection,
    app_state: &AppState,
) -> FdPoller {
    match FdPoller::new(conn, app_state.input_context.as_ref()) {
        Ok(poller) => poller,
        Err(FdPollerCreationError::WaylandConnection(e)) => {
            log::error!("Failed to create FdPoller due to Wayland connection error: {}. Exiting.", e);
            process::exit(1);
        }
    }
}

pub fn log_input_device_status(app_config: &AppConfig, input_context_is_some: bool) {
    if !app_config.key.is_empty() && input_context_is_some {
        log::info!(
            "Libinput context was initialized. If keys do not respond, please check previous log messages \
            for any 'Failed to open path' errors from the input system. These errors often indicate \
            permission issues (e.g., the user running the application may not be in the 'input' group)."
        );
    } else if !app_config.key.is_empty() && !input_context_is_some {
        log::warn!(
            "Key input is configured in keys.toml, but the libinput context could not be initialized \
            (see previous errors). Key press/release events will not be monitored."
        );
    }
}
