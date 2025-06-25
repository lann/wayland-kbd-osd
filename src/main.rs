// Wayland Client Imports

// Standard Library Imports
use std::io;
// RawFd and AsRawFd are no longer directly used in main after FdPoller changes
use std::process; // Used in main loop for poll error

// Crate-specific modules
mod check; // Added new module
mod config;
mod draw; // Not directly used in main, but AppState::draw calls it
mod event;
mod keycodes;
mod poll_fds; // Added new module
mod setup; // Added new module
mod text_utils; // Added new module
mod wayland;
mod wayland_drawing_cache; // Added new module

// External Crate Imports
use clap::Parser;

// Using items from the new modules
use config::{load_and_process_config, AppConfig};
// MyLibinputInterface is used by setup module
// handle_libinput_events is called via event::handle_libinput_events
// handle_wayland_events is called via wayland::handle_wayland_events
use wayland::AppState;
use crate::poll_fds::{PollEvent, PollError}; // Using the new polling module

// Protocol imports for main function logic (surface creation, layer shell)
// WaylandError is used in main loop
use wayland_client::backend::WaylandError;
// wl_output and zwlr_layer_shell_v1 are used by setup module
// Connection is not directly used in main.rs anymore

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

    /// Run in window mode instead of the default overlay mode
    #[clap(long)]
    window: bool,

    /// Set the window background color (e.g., #RRGGBBAA, #RGB)
    #[clap(long, default_value = "#000000FF")]
    window_color: String,
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
        // Call the run_check function from the new check module
        check::run_check(&cli.config_path, &app_config);
        // run_check will process::exit(0) on success or process::exit(1) on error.
    }

    log::info!(
        "Starting Wayland application with config '{}'...",
        &cli.config_path
    );
    log::info!("Configuration loaded and processed successfully.");

    let conn = setup::initialize_wayland_connection();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let parsed_window_color = match config::parse_color_string(&cli.window_color) {
        Ok(color) => color,
        Err(e) => {
            eprintln!(
                "Error parsing --window-color value '{}': {}. Using default black.",
                cli.window_color, e
            );
            config::parse_color_string("#000000FF").unwrap()
        }
    };

    let mut app_state = AppState::new(app_config.clone(), cli.window, parsed_window_color);

    // Bind Wayland globals (compositor, shm, xdg_wm_base, layer_shell, outputs)
    let _registry = conn.display().get_registry(&qh, ()); // Get registry to trigger global events
    setup::initialize_globals_and_outputs(&conn, &mut event_queue, &mut app_state);

    // Initialize libinput
    setup::initialize_libinput_context(&mut app_state);

    // Create Wayland surface
    setup::create_wayland_surface(&mut app_state, &qh);

    // Setup overlay or window mode
    if !app_state.is_window_mode { // If not explicitly window mode, try overlay
        setup::setup_overlay_mode(&mut app_state, &qh);
    } else { // Explicitly window mode or fallback from overlay
        setup::setup_window_mode(&mut app_state, &qh);
    }

    // Finalize surface setup (commit, roundtrip, initial size, frame callback)
    setup::finalize_surface_setup(&mut event_queue, &mut app_state, &qh);

    log::info!("Entering main event loop.");

    // Initialize FdPoller
    let mut fd_poller = setup::initialize_fd_poller(&conn, &app_state);

    // Log input device status
    setup::log_input_device_status(&app_state.config, app_state.input_context.is_some());

    let poll_timeout_ms = 33;
    let mut libinput_active = app_state.input_context.is_some();

    while app_state.running {
        match fd_poller.poll(poll_timeout_ms) {
            Ok(events) => {
                for event_item in events { // Renamed to avoid conflict with crate::event
                    match event_item {
                        PollEvent::WaylandReady => {
                            if wayland::handle_wayland_events(&conn, &mut event_queue, &mut app_state).is_err() {
                                app_state.running = false;
                                break;
                            }
                        }
                        PollEvent::LibinputReady => {
                            if libinput_active {
                                event::handle_libinput_events(&mut app_state);
                            }
                        }
                        PollEvent::WaylandError => {
                            log::error!("Wayland FD error/hangup reported by FdPoller. Exiting.");
                            app_state.running = false;
                            break;
                        }
                        PollEvent::LibinputError => {
                            log::error!("Libinput FD error/hangup reported by FdPoller. Input monitoring will stop.");
                            if let Some(ref mut context) = app_state.input_context {
                                let _ = context.dispatch(); // Attempt to clear pending
                            }
                            app_state.input_context = None; // Mark libinput as inactive

                            // Recreate FdPoller without libinput - use the setup function
                            fd_poller = setup::initialize_fd_poller(&conn, &app_state);
                            libinput_active = false; // Already false due to app_state.input_context = None
                            log::warn!("Libinput context removed due to FD error. Key press/release events will no longer be monitored.");
                        }
                        PollEvent::Timeout => {
                            // Timeout is fine
                        }
                    }
                    if !app_state.running { // Check if an event handler set running to false
                        break;
                    }
                }
            }
            Err(PollError::Interrupted) => {
                // EINTR, just continue
                continue;
            }
            Err(PollError::Other(errno)) => {
                log::error!("FdPoller returned error: {} (errno {}). Exiting.", io::Error::from_raw_os_error(errno), errno);
                app_state.running = false;
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
            } else {
                log::warn!("Main loop: needs_redraw is true, but surface is None. Cannot request frame callback.");
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
// log_if_input_device_access_denied was moved to setup.rs as log_input_device_status
