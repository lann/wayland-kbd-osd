use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::protocol::{wl_compositor, wl_shm, wl_shm_pool, wl_surface, wl_buffer, wl_registry};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Graphics and Font rendering
use raqote::{SolidSource, PathBuilder, DrawOptions, StrokeStyle, Transform, Source}; // Removed DrawTarget
use rusttype::{Font, Scale, point, PositionedGlyph, OutlineBuilder};
use euclid::Angle; // Import Angle

// Struct to hold key properties
struct KeyDisplay {
    text: String,
    center_x: f32,
    center_y: f32,
    width: f32,
    height: f32,
    corner_radius: f32,
    border_thickness: f32,
    rotation_degrees: f32,
    text_size: f32,
    border_color: SolidSource,
    background_color: SolidSource,
    text_color: SolidSource,
}

fn draw_single_key(
    dt: &mut raqote::DrawTarget,
    key: &KeyDisplay,
    font: &Font<'_>
) {
    // Calculate text metrics first (needed for centering)
    let scale = Scale::uniform(key.text_size);
    let v_metrics = font.v_metrics(scale);
    let glyphs: Vec<PositionedGlyph<'_>> = font
        .layout(&key.text, scale, point(0.0, 0.0 + v_metrics.ascent))
        .collect();

    let text_width_pixels = glyphs
        .iter()
        .rev()
        .filter_map(|g| g.pixel_bounding_box().map(|bb| bb.max.x as f32))
        .next()
        .unwrap_or(0.0);

    // Create a transform for this specific key
    let transform = Transform::translation(key.center_x, key.center_y)
        .then_rotate(Angle::radians(key.rotation_degrees.to_radians()))
        .then_translate(raqote::Vector::new(-key.width / 2.0, -key.height / 2.0));

    dt.set_transform(&transform);

    // Draw rounded rectangle (background)
    let mut pb = PathBuilder::new(); // Create a new path builder for each key's geometry
    pb.move_to(0.0 + key.corner_radius, 0.0);
    pb.line_to(key.width - key.corner_radius, 0.0);
    pb.quad_to(key.width, 0.0, key.width, key.corner_radius);
    pb.line_to(key.width, key.height - key.corner_radius);
    pb.quad_to(key.width, key.height, key.width - key.corner_radius, key.height);
    pb.line_to(key.corner_radius, key.height);
    pb.quad_to(0.0, key.height, 0.0, key.height - key.corner_radius);
    pb.line_to(0.0, key.corner_radius);
    pb.quad_to(0.0, 0.0, key.corner_radius, 0.0);
    pb.close();
    let key_path = pb.finish();

    dt.fill(&key_path, &Source::Solid(key.background_color), &DrawOptions::default());

    // Draw rounded rectangle (border)
    dt.stroke(
        &key_path,
        &Source::Solid(key.border_color),
        &StrokeStyle {
            width: key.border_thickness,
            ..Default::default()
        },
        &DrawOptions::default(),
    );

    // Draw text
    let text_x = (key.width - text_width_pixels) / 2.0;
    let text_y = (key.height - key.text_size) / 2.0 + v_metrics.ascent;
    let text_transform = transform.then_translate(raqote::Vector::new(text_x, text_y));
    dt.set_transform(&text_transform);

    for glyph_instance in glyphs {
        let mut glyph_pb = PathBuilder::new(); // Create a new PathBuilder for each glyph
        if glyph_instance.unpositioned().build_outline(&mut PathBuilderSink(&mut glyph_pb)) {
            let glyph_path = glyph_pb.finish();
            if !glyph_path.ops.is_empty() {
                dt.fill(&glyph_path, &Source::Solid(key.text_color), &DrawOptions::default());
            }
        }
    }
    // The main draw loop will reset the transform once after all keys (or other elements) are drawn.
}


// Remove unused File and Write
use std::os::unix::io::{AsRawFd, BorrowedFd};
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

        let temp_file = tempfile::tempfile().expect("Failed to create temp file");
        temp_file.set_len(size as u64).expect("Failed to set temp file length");

        let fd = temp_file.as_raw_fd();
        let pool = unsafe { shm.create_pool(BorrowedFd::borrow_raw(fd), size, qh, ()) };
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();

        let mut mmap = unsafe { MmapMut::map_mut(&temp_file).expect("Failed to map temp file") };
        let mut dt = raqote::DrawTarget::new(width, height);

        // Clear the drawing target once
        dt.clear(SolidSource::from_unpremultiplied_argb(0x00, 0x00, 0x00, 0x00));

        // Load font once
        let font_data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        let font = Font::try_from_bytes(font_data as &[u8]).expect("Error constructing Font");

        // Define common properties for keys, can be customized per key
        let key_w: f32 = 80.0; // Slightly smaller keys to fit more
        let key_h: f32 = 40.0;
        let corner_r: f32 = 8.0;
        let border_t: f32 = 2.0;
        let default_rot: f32 = 30.0; // Less rotation to manage space
        let txt_size: f32 = 18.0; // Smaller text

        let border_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x80, 0x80, 0x80); // Grey
        let background_c = SolidSource::from_unpremultiplied_argb(0xFF, 0xE0, 0xE0, 0xE0); // Lighter grey
        let text_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x10, 0x10, 0x10); // Darker Black

        let keys_to_draw = vec![
            KeyDisplay {
                text: "Alt".to_string(),
                center_x: width as f32 * 0.70,
                center_y: height as f32 * 0.60,
                width: key_w,
                height: key_h,
                corner_radius: corner_r,
                border_thickness: border_t,
                rotation_degrees: default_rot,
                text_size: txt_size,
                border_color: border_c,
                background_color: background_c,
                text_color: text_c,
            },
            KeyDisplay {
                text: "Ctrl".to_string(),
                center_x: width as f32 * 0.30,
                center_y: height as f32 * 0.40,
                width: key_w,
                height: key_h,
                corner_radius: corner_r,
                border_thickness: border_t,
                rotation_degrees: default_rot - 5.0, // Slightly different rotation
                text_size: txt_size,
                border_color: border_c,
                background_color: background_c,
                text_color: text_c,
            },
        ];

        for key_spec in keys_to_draw {
            draw_single_key(&mut dt, &key_spec, &font);
        }

        // Reset transform after all drawing operations on dt that use transforms
        dt.set_transform(&Transform::identity());

        let dt_buffer = dt.get_data_u8();
        for y_idx in 0..height {
            for x_idx in 0..width {
                let dt_buf_idx = (y_idx * width + x_idx) as usize * 4;
                let mmap_buf_idx = (y_idx * stride + x_idx * 4) as usize;
                if dt_buf_idx + 3 < dt_buffer.len() && mmap_buf_idx + 3 < mmap.len() {
                    let a = dt_buffer[dt_buf_idx + 3]; // Raqote is BGRA in get_data_u8, so A is at +3
                    let r = dt_buffer[dt_buf_idx + 2]; // R is at +2
                    let g = dt_buffer[dt_buf_idx + 1]; // G is at +1
                    let b = dt_buffer[dt_buf_idx + 0]; // B is at +0
                    // Wayland expects ARGB8888, which means u32 with A in MSB.
                    // On little-endian, memory layout for 0xAARRGGBB u32 is BB GG RR AA.
                    // Raqote provides BGRA bytes: B, G, R, A.
                    // So, we want to write [B, G, R, A] into mmap.
                    // pixel_value from these bytes should be (A<<24)|(R<<16)|(G<<8)|B
                    let pixel_value = (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32);
                    mmap[mmap_buf_idx..mmap_buf_idx+4].copy_from_slice(&pixel_value.to_le_bytes());
                }
            }
        }

        log::info!("Drawing content (rotated 'Alt' key)");
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width, height);
        surface.commit();
        self.buffer = Some(buffer);
        self.mmap = Some(mmap);
    }
}

// Helper for rusttype to raqote path conversion
struct PathBuilderSink<'a>(&'a mut raqote::PathBuilder);

impl<'a> OutlineBuilder for PathBuilderSink<'a> { // Ensure this uses rusttype::OutlineBuilder
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
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
