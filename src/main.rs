use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::protocol::{wl_compositor, wl_shm, wl_shm_pool, wl_surface, wl_buffer, wl_registry}; // wl_buffer is already here
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Remove unused File and Write
use std::os::unix::io::{AsRawFd, BorrowedFd}; // Added BorrowedFd
use memmap2::MmapMut;

const WINDOW_WIDTH: i32 = 320;
const WINDOW_HEIGHT: i32 = 240;

struct AppState {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    surface: Option<wl_surface::WlSurface>,
    buffer: Option<wl_buffer::WlBuffer>,
    mmap: Option<MmapMut>,
    configured_width: i32,
    configured_height: i32,
}

impl AppState {
    fn new() -> Self {
        AppState {
            compositor: None,
            shm: None,
            xdg_wm_base: None,
            surface: None,
            buffer: None,
            mmap: None,
            configured_width: WINDOW_WIDTH, // Default
            configured_height: WINDOW_HEIGHT, // Default
        }
    }

    fn draw(&mut self, qh: &QueueHandle<AppState>) {
        if self.surface.is_none() || self.shm.is_none() || self.compositor.is_none() {
            log::error!("Cannot draw: missing essential Wayland objects.");
            return;
        }

        let surface = self.surface.as_ref().unwrap();
        let shm = self.shm.as_ref().unwrap();

        let width = self.configured_width;
        let height = self.configured_height;
        let stride = width * 4; // 4 bytes per pixel (ARGB8888)
        let size = stride * height;

        // Create a temporary file for shared memory
        let temp_file = tempfile::tempfile().expect("Failed to create temp file"); // Removed mut
        temp_file.set_len(size as u64).expect("Failed to set temp file length");

        let fd = temp_file.as_raw_fd(); // Get raw fd first
        let pool = unsafe { shm.create_pool(BorrowedFd::borrow_raw(fd), size, qh, ()) }; // Use BorrowedFd, needs unsafe for borrow_raw

        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy(); // Pool can be destroyed after buffer creation

        // Memory map the file
        let mut mmap = unsafe { MmapMut::map_mut(&temp_file).expect("Failed to map temp file") };

        // Draw a blue rectangle with a red border
        for y in 0..height {
            for x in 0..width {
                let offset = (y * stride + x * 4) as usize;
                let color: u32 = if x < 5 || x >= width - 5 || y < 5 || y >= height - 5 {
                    0xFFFF0000 // Red border (ARGB)
                } else {
                    0xFF0000FF // Blue rectangle (ARGB)
                };
                mmap[offset..offset+4].copy_from_slice(&color.to_le_bytes());
            }
        }

        // For "hello world", we'll just print to console for now, as rendering text is complex.
        log::info!("Drawing content (rectangle + 'Hello World' placeholder)");

        // Attach the buffer and commit
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width, height); // Mark the whole buffer as damaged
        surface.commit();

        // Store buffer and mmap so they are not dropped immediately
        // In a real app, you'd manage buffers more carefully (e.g., double buffering)
        self.buffer = Some(buffer);
        self.mmap = Some(mmap); // Keep mmap alive
    }
}


impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
    fn event(
        _state: &mut AppState,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
    fn event(
        state: &mut AppState,
        surface_proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            surface_proxy.ack_configure(serial);
            // Dimensions are now expected to be set by xdg_toplevel configure.
            // This configure event is the trigger to draw with the new state.
            if state.surface.is_some() {
                 state.draw(qh); // draw will use state.configured_width/height
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for AppState {
    fn event(
        state: &mut AppState,
        _proxy: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                log::debug!("XDG Toplevel Configure: width: {}, height: {}, states: {:?}", width, height, states);
                // Use provided dimensions if > 0, otherwise keep existing (or default if first configure)
                if width > 0 { state.configured_width = width; }
                if height > 0 { state.configured_height = height; }
                // Note: A more robust handling might involve checking if the surface is maximized, etc.
                // and then deciding whether to use suggested size or a client-preferred one.
            }
            xdg_toplevel::Event::Close => {
                // Handle window close
                log::info!("XDG Toplevel Close event received. Application should exit.");
                // TODO: Implement graceful exit for the application loop.
            }
            _ => { /* log::trace!("Unhandled xdg_toplevel event: {:?}", event); */ }
        }
    }
}

// Corrected Dispatch signatures for wl_compositor, wl_surface, wl_shm
impl Dispatch<wl_compositor::WlCompositor, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _proxy: &wl_compositor::WlCompositor,
        _event: wl_compositor::Event,
        _udata: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // No compositor events are usually handled by clients
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _proxy: &wl_surface::WlSurface,
        _event: wl_surface::Event,
        _udata: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // Surface events (like enter, leave) can be handled here if needed
    }
}

impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _proxy: &wl_shm::WlShm,
        _event: wl_shm::Event,
        _udata: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // WL_SHM events (like format) can be handled here
    }
}

// Required Dispatch implementations based on compiler errors
impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut AppState,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _udata: &(),
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => {
                    let capped_version = std::cmp::min(version, 5); // Cap version for safety
                    state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, (), AppState>(name, capped_version, qh, ()));
                }
                "wl_shm" => {
                    let capped_version = std::cmp::min(version, 1);
                    state.shm = Some(registry.bind::<wl_shm::WlShm, (), AppState>(name, capped_version, qh, ()));
                }
                "xdg_wm_base" => {
                    let capped_version = std::cmp::min(version, 3);
                    let wm_base = registry.bind::<xdg_wm_base::XdgWmBase, (), AppState>(name, capped_version, qh, ());
                    // Ping handling is done by AppState's Dispatch impl for XdgWmBase
                    state.xdg_wm_base = Some(wm_base);
                }
                _ => { /* println!("Ignoring global: {} v{}", interface, version); */ }
            }
        } else if let wl_registry::Event::GlobalRemove { name } = event {
            // This 'name' is the numeric ID of the global. Proper removal requires mapping this ID
            // back to the objects stored in AppState, which can be complex.
            log::info!("Global removed: ID {}", name);
            // TODO: Implement logic to unbind or mark as None if a managed global is removed.
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _proxy: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        _udata: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // No events from wl_shm_pool are expected by clients normally
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _state: &mut AppState,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _udata: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        if let wl_buffer::Event::Release = event {
            log::debug!("Buffer {:?} released", buffer.id());
            // In a real app, you would mark this buffer as free or destroy it.
            // If this buffer is state.buffer, we might want to clear it:
            // if state.buffer.as_ref().map_or(false, |b| b == buffer) {
            //     state.buffer = None;
            //     state.mmap = None; // And drop the mmap
            // }
        }
    }
}

fn main() {
    env_logger::init();
    log::info!("Starting Wayland application...");

    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut app_state = AppState::new();

    let display = conn.display();
    // Request the registry. Events (globals) will be dispatched to AppState via its
    // Dispatch<wl_registry::WlRegistry, ()> implementation.
    let _registry = display.get_registry(&qh, ());

    // First roundtrip to process globals and other initial events.
    // The AppState's Dispatch<wl_registry::WlRegistry> will bind globals.
    log::info!("Processing initial events for globals...");
    event_queue.roundtrip(&mut app_state).unwrap();

    // Check if essential globals were bound by the Dispatch implementation
    if app_state.compositor.is_none() || app_state.shm.is_none() || app_state.xdg_wm_base.is_none() {
        log::error!("Failed to bind essential Wayland globals. Compositor: {:?}, SHM: {:?}, XDG WM Base: {:?}", app_state.compositor.is_some(), app_state.shm.is_some(), app_state.xdg_wm_base.is_some());
        panic!("Failed to bind essential Wayland globals. Check Dispatch<wl_registry::WlRegistry> logic and compositor logs.");
    }
    log::info!("Essential globals bound.");

    let surface = app_state.compositor.as_ref().unwrap().create_surface(&qh, ());
    app_state.surface = Some(surface.clone());

    let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("Hello Wayland Rectangle".to_string());

    surface.commit(); // Make the surface known.

    log::info!("Wayland window configured. Waiting for events (like configure to draw)...");

    loop {
        event_queue.blocking_dispatch(&mut app_state).unwrap();
    }
}
