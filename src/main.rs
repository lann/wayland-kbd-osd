// Wayland Client Imports
use wayland_client::Connection;

// Standard Library Imports
use std::io;
use std::os::unix::io::{AsRawFd as _, RawFd}; // Used in main loop
use std::process; // Used in main loop for poll error

// Crate-specific modules
mod config;
mod draw; // Not directly used in main, but AppState::draw calls it
mod event;
mod keycodes;
mod wayland;

// External Crate Imports
use clap::Parser;
use freetype::Library as FreeTypeLibrary; // Used for --check

// Using items from the new modules
use config::{
    load_and_process_config,
    print_overlay_config_for_check,
    simulate_text_layout, // TextCheckResult is pub from config but not used directly here
    validate_config,
    AppConfig,
    DEFAULT_TEXT_SIZE_UNSCALED,
};
use event::MyLibinputInterface;
// handle_libinput_events is called via event::handle_libinput_events
// handle_wayland_events is called via wayland::handle_wayland_events
use wayland::AppState;

// Protocol imports for main function logic (surface creation, layer shell)
use wayland_client::backend::WaylandError;
use wayland_client::protocol::wl_output; // For selected_wl_output_proxy
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

/// Command-line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Check the configuration file for errors and print layout information
    #[clap(long)]
    check: bool,

    /// Path to the configuration file
    #[clap(long, value_parser, default_value = "keys.toml")]
    config_path: String,

    /// Run in overlay mode (requires compositor support for wlr-layer-shell)
    #[clap(long)]
    overlay: bool,
}

fn main() {
    let cli = Cli::parse();

    if let Some(e) =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init()
            .err()
    {
        eprintln!(
            "Failed to initialize logger: {}. Continuing without detailed logging for --check.",
            e
        )
    }

    let app_config: AppConfig = match load_and_process_config(&cli.config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    if cli.check {
        println!(
            "Performing configuration check for '{}'...",
            &cli.config_path
        );

        if let Err(e) = validate_config(&app_config) {
            eprintln!("Configuration validation failed: {}", e);
            process::exit(1);
        } else {
            println!("Basic validation (overlaps, duplicates, positive dimensions) passed.");
        }

        let font_data: &[u8] = include_bytes!("../default-font/DejaVuSansMono.ttf");
        let ft_library = match FreeTypeLibrary::init() {
            Ok(lib) => lib,
            Err(e) => {
                eprintln!("Failed to initialize FreeType library for --check: {:?}", e);
                process::exit(1);
            }
        };
        let ft_face = match ft_library.new_memory_face(font_data.to_vec(), 0) {
            Ok(face) => face,
            Err(e) => {
                eprintln!("Failed to load font for --check: {:?}", e);
                process::exit(1);
            }
        };

        println!("\nKey Information (Layout from TOML, Text metrics simulated):");
        println!(
            "{:<20} | {:<25} | {:<10} | {:<10} | {:<20}",
            "Label (Name)", "Bounding Box (L,T,R,B)", "Keycode", "Font Scale", "Truncated Label"
        );
        println!(
            "{:-<20}-+-{:-<25}-+-{:-<10}-+-{:-<10}-+-{:-<20}",
            "", "", "", "", ""
        );

        for key_config_item in &app_config.key {
            let right_edge = key_config_item.left + key_config_item.width;
            let bottom_edge = key_config_item.top + key_config_item.height;
            let bbox_str = format!(
                "{:.1},{:.1}, {:.1},{:.1}",
                key_config_item.left, key_config_item.top, right_edge, bottom_edge
            );

            let initial_font_size = key_config_item
                .text_size
                .unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64;

            match simulate_text_layout(key_config_item, &ft_face) {
                // This now correctly uses config::simulate_text_layout
                Ok(text_check_result) => {
                    // text_check_result is config::TextCheckResult
                    let font_scale = if initial_font_size > 0.0 {
                        text_check_result.final_font_size_pts / initial_font_size
                    } else {
                        1.0
                    };

                    let truncated_label_display = if text_check_result.truncated_chars > 0
                        || !text_check_result.final_text.eq(&key_config_item.name)
                    {
                        text_check_result.final_text
                    } else {
                        "".to_string()
                    };

                    println!(
                        "{:<20} | {:<25} | {:<10} | {:<10.2} | {:<20}",
                        key_config_item.name,
                        bbox_str,
                        key_config_item.keycode,
                        font_scale,
                        truncated_label_display
                    );
                }
                Err(e) => {
                    println!(
                        "{:<20} | {:<25} | {:<10} | {:<10.2} | Error simulating text: {} ",
                        key_config_item.name, bbox_str, key_config_item.keycode, 1.0, e
                    );
                }
            }
        }

        print_overlay_config_for_check(&app_config.overlay); // This now correctly uses config::print_overlay_config_for_check

        println!("\nConfiguration check finished.");
        process::exit(0);
    }

    log::info!(
        "Starting Wayland application with config '{}'...",
        &cli.config_path
    );
    log::info!("Configuration loaded and processed successfully.");

    let conn = Connection::connect_to_env().unwrap_or_else(|e| {
        log::error!("Failed to connect to Wayland display: {}", e);
        eprintln!("Failed to connect to Wayland display. Is a Wayland compositor running?");
        process::exit(1);
    });

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut app_state = AppState::new(app_config.clone());

    let _registry = conn.display().get_registry(&qh, ());

    log::trace!("Dispatching initial events to bind globals and get initial output info...");
    for _ in 0..3 {
        if event_queue.dispatch_pending(&mut app_state).is_err() {
            log::error!("Error dispatching events during initial setup.");
            break;
        }
        if conn.flush().is_err() {
            log::error!("Error flushing connection during initial setup.");
            break;
        }
    }
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during final initial roundtrip for global binding.");
    }

    if app_state.compositor.is_none() || app_state.shm.is_none() || app_state.xdg_wm_base.is_none()
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

    let surface = app_state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    app_state.surface = Some(surface.clone());

    if cli.overlay {
        log::info!("Overlay mode requested. Attempting to use wlr-layer-shell.");
        if let Some(layer_shell) = app_state.layer_shell.as_ref() {
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
                            app_state.identified_target_wl_output_name =
                                Some(*output_registry_name);
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
                &surface,
                selected_wl_output_proxy,
                zwlr_layer_shell_v1::Layer::Overlay,
                "wayland-kbd-osd".to_string(),
                &qh,
                (),
            );

            let base_anchor = match app_state.config.overlay.position {
                config::OverlayPosition::Top | config::OverlayPosition::TopCenter => {
                    zwlr_layer_surface_v1::Anchor::Top
                        | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right
                }
                config::OverlayPosition::Bottom | config::OverlayPosition::BottomCenter => {
                    zwlr_layer_surface_v1::Anchor::Bottom
                        | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right
                }
                config::OverlayPosition::Left => zwlr_layer_surface_v1::Anchor::Left,
                config::OverlayPosition::Right => zwlr_layer_surface_v1::Anchor::Right,
                config::OverlayPosition::Center => {
                    zwlr_layer_surface_v1::Anchor::Top
                        | zwlr_layer_surface_v1::Anchor::Bottom
                        | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right
                }
                config::OverlayPosition::TopLeft => {
                    zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left
                }
                config::OverlayPosition::TopRight => {
                    zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Right
                }
                config::OverlayPosition::BottomLeft => {
                    zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Left
                }
                config::OverlayPosition::BottomRight => {
                    zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Right
                }
                config::OverlayPosition::CenterLeft => zwlr_layer_surface_v1::Anchor::Left,
                config::OverlayPosition::CenterRight => zwlr_layer_surface_v1::Anchor::Right,
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

            layer_surface_obj
                .set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
            layer_surface_obj.set_exclusive_zone(0);

            log::info!("Setting initial layer surface size to (1,1). Actual size will be configured once screen dimensions are known.");
            layer_surface_obj.set_size(1, 1);

            app_state.layer_surface = Some(layer_surface_obj);
            log::info!("Created and configured layer surface for overlay mode.");
        } else {
            log::error!("--overlay flag was used, but zwlr_layer_shell_v1 is not available from the compositor. Falling back to normal window mode.");
            let xdg_surface_proxy =
                app_state
                    .xdg_wm_base
                    .as_ref()
                    .unwrap()
                    .get_xdg_surface(&surface, &qh, ());
            let toplevel = xdg_surface_proxy.get_toplevel(&qh, ());
            toplevel.set_title("Wayland Keyboard OSD (Fallback)".to_string());
        }
    } else {
        log::info!("Normal window mode requested (XDG shell).");
        let xdg_surface_proxy =
            app_state
                .xdg_wm_base
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface_proxy.get_toplevel(&qh, ());
        toplevel.set_title("Wayland Keyboard OSD".to_string());
    }

    surface.commit();

    log::info!("Initial surface commit done. Dispatching events to catch initial configure...");
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during roundtrip after surface commit (waiting for initial configure).");
    }

    if cli.overlay && app_state.layer_shell.is_some() {
        log::info!("Initial layer surface setup complete. Attempting to configure size based on currently known screen dimensions...");
        app_state.attempt_configure_layer_surface_size();

        if !app_state.initial_surface_size_set {
            log::warn!("Initial attempt to set layer surface size deferred as screen dimensions are not yet known or not valid for the target output. Will retry upon receiving output events.");
        }
    }

    log::info!("Initial setup phase complete. Wayland window should be configured or awaiting configuration. Waiting for events...");

    // Ensure the drawing loop starts if initial configuration didn't trigger a draw and callback.
    if app_state.needs_redraw && app_state.frame_callback.is_none() {
        if let Some(surface) = app_state.surface.as_ref() {
            log::debug!("Main setup: needs_redraw is true and no frame_callback pending, requesting initial frame callback.");
            let callback = surface.frame(&qh, ());
            app_state.frame_callback = Some(callback);
        } else {
            log::warn!("Main setup: needs_redraw is true, but surface is None. Cannot request initial frame callback.");
        }
    }

    log::info!("Entering main event loop.");

    let wayland_raw_fd: RawFd = match conn.prepare_read() {
        Ok(guard) => guard.connection_fd().as_raw_fd(),
        Err(e) => {
            log::error!(
                "Failed to prepare_read Wayland connection before starting event loop: {}",
                e
            );
            process::exit(1);
        }
    };

    let mut fds: Vec<libc::pollfd> = Vec::new();
    fds.push(libc::pollfd {
        fd: wayland_raw_fd,
        events: libc::POLLIN,
        revents: 0,
    });
    const WAYLAND_FD_IDX: usize = 0;

    let mut libinput_fd_idx_opt: Option<usize> = None;
    if let Some(ref context) = app_state.input_context {
        let libinput_raw_fd: RawFd = context.as_raw_fd();
        fds.push(libc::pollfd {
            fd: libinput_raw_fd,
            events: libc::POLLIN,
            revents: 0,
        });
        libinput_fd_idx_opt = Some(fds.len() - 1);
        log_if_input_device_access_denied(&app_state.config, true);
    } else {
        log_if_input_device_access_denied(&app_state.config, false);
        log::warn!(
            "No libinput context available. Key press/release events will not be monitored."
        );
    }

    let poll_timeout_ms = 33;

    while app_state.running {
        for item in fds.iter_mut() {
            item.revents = 0;
        }

        let ret =
            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, poll_timeout_ms) };

        if ret < 0 {
            let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::EINTR {
                continue;
            }
            log::error!("libc::poll error: {}", io::Error::last_os_error());
            app_state.running = false;
            break;
        } else if ret == 0 {
            // Timeout
        } else {
            if (fds[WAYLAND_FD_IDX].revents & libc::POLLIN) != 0
                && wayland::handle_wayland_events(&conn, &mut event_queue, &mut app_state).is_err()
            {
                break;
            }
            if (fds[WAYLAND_FD_IDX].revents & (libc::POLLERR | libc::POLLHUP)) != 0 {
                log::error!("Wayland FD error/hangup (POLLERR/POLLHUP). Exiting.");
                app_state.running = false;
            }

            if let Some(libinput_idx) = libinput_fd_idx_opt {
                if app_state.running && (fds[libinput_idx].revents & libc::POLLIN) != 0 {
                    event::handle_libinput_events(&mut app_state);
                }
                if app_state.running
                    && (fds[libinput_idx].revents & (libc::POLLERR | libc::POLLHUP)) != 0
                {
                    log::error!(
                        "Libinput FD error/hangup (POLLERR/POLLHUP). Input monitoring might stop."
                    );
                    if let Some(ref mut context) = app_state.input_context {
                        let _ = context.dispatch();
                    }
                    app_state.input_context = None;

                    if let Some(idx_to_remove) = fds
                        .iter()
                        .position(|pollfd| pollfd.fd == fds[libinput_idx].fd)
                    {
                        fds.remove(idx_to_remove);
                    }
                    libinput_fd_idx_opt = None;

                    log::warn!("Libinput context removed due to FD error. Key press/release events will no longer be monitored.");
                }
            }
        }

        if !app_state.running {
            break;
        }

        if app_state.needs_redraw && app_state.frame_callback.is_none() {
            if let Some(surface) = app_state.surface.as_ref() {
                log::debug!("Main loop: needs_redraw is true and no frame_callback pending, requesting frame callback.");
                let callback = surface.frame(&qh, ());
                app_state.frame_callback = Some(callback);
                // The actual draw will happen when the frame callback fires.
                // needs_redraw remains true until the draw in the callback.
            } else {
                log::warn!("Main loop: needs_redraw is true, but surface is None. Cannot request frame callback.");
                // If there's no surface, we can't request a callback.
                // We might set needs_redraw to false to prevent spamming this log,
                // or rely on other parts of the code to re-set it when a surface appears.
                // For now, let it remain true, as the callback handler also checks for surface.
            }
        }

        match conn.flush() {
            Ok(_) => {}
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => { /* Fine */ }
            Err(e) => {
                log::error!("Failed to flush Wayland connection: {}", e);
                app_state.running = false;
            }
        }
    }
    log::info!("Exiting application loop.");
}

fn log_if_input_device_access_denied(app_config_ref: &AppConfig, input_context_is_some: bool) {
    if !app_config_ref.key.is_empty() && input_context_is_some {
        log::info!(
            "Libinput context was initialized. If keys do not respond, please check previous log messages \
            for any 'Failed to open path' errors from the input system. These errors often indicate \
            permission issues (e.g., the user running the application may not be in the 'input' group)."
        );
    } else if !app_config_ref.key.is_empty() && !input_context_is_some {
        log::warn!(
            "Key input is configured in keys.toml, but the libinput context could not be initialized \
            (see previous errors). Key press/release events will not be monitored."
        );
    }
}
