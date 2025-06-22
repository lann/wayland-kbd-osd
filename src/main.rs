use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::protocol::{wl_compositor, wl_shm, wl_shm_pool, wl_surface, wl_buffer, wl_registry};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use input; // Added for libinput

// Graphics and Font rendering
use raqote::{SolidSource, PathBuilder, DrawOptions, StrokeStyle, Transform, Source};
use rusttype::{Font, Scale, point, PositionedGlyph, OutlineBuilder};
use euclid::Angle;

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

    let transform = Transform::translation(key.center_x, key.center_y)
        .then_rotate(Angle::radians(key.rotation_degrees.to_radians()))
        .then_translate(raqote::Vector::new(-key.width / 2.0, -key.height / 2.0));

    dt.set_transform(&transform);

    let mut pb = PathBuilder::new();
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
    dt.stroke(
        &key_path,
        &Source::Solid(key.border_color),
        &StrokeStyle {
            width: key.border_thickness,
            ..Default::default()
        },
        &DrawOptions::default(),
    );

    let text_x = (key.width - text_width_pixels) / 2.0;
    let text_y = (key.height - key.text_size) / 2.0 + v_metrics.ascent;
    let text_transform = transform.then_translate(raqote::Vector::new(text_x, text_y));
    dt.set_transform(&text_transform);

    for glyph_instance in glyphs {
        let mut glyph_pb = PathBuilder::new();
        if glyph_instance.unpositioned().build_outline(&mut PathBuilderSink(&mut glyph_pb)) {
            let glyph_path = glyph_pb.finish();
            if !glyph_path.ops.is_empty() {
                dt.fill(&glyph_path, &Source::Solid(key.text_color), &DrawOptions::default());
            }
        }
    }
}

use std::os::unix::io::{AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::fs::OpenOptions;
use std::path::Path;
use libc::{O_RDWR, O_NONBLOCK};

use memmap2::MmapMut;

const WINDOW_WIDTH: i32 = 320;
const WINDOW_HEIGHT: i32 = 240;

struct MyLibinputInterface;

impl input::LibinputInterface for MyLibinputInterface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        log::debug!("Opening path: {:?}, flags: {}", path, flags);
        OpenOptions::new()
            .custom_flags(O_RDWR | O_NONBLOCK)
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

struct AppState {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    surface: Option<wl_surface::WlSurface>,
    buffer: Option<wl_buffer::WlBuffer>,
    mmap: Option<MmapMut>,
    configured_width: i32,
    configured_height: i32,
    input_context: Option<input::Libinput>,
    left_ctrl_pressed: bool,
    left_alt_pressed: bool,
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
            configured_width: WINDOW_WIDTH,
            configured_height: WINDOW_HEIGHT,
            input_context: None,
            left_ctrl_pressed: false,
            left_alt_pressed: false,
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
        let stride = width * 4;
        let size = stride * height;

        let temp_file = tempfile::tempfile().expect("Failed to create temp file");
        temp_file.set_len(size as u64).expect("Failed to set temp file length");

        let fd = temp_file.as_raw_fd();
        let pool = unsafe { shm.create_pool(BorrowedFd::borrow_raw(fd), size, qh, ()) };
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();

        let mut mmap = unsafe { MmapMut::map_mut(&temp_file).expect("Failed to map temp file") };
        let mut dt = raqote::DrawTarget::new(width, height);

        dt.clear(SolidSource::from_unpremultiplied_argb(0x00, 0x00, 0x00, 0x00));

        let font_data = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        let font = Font::try_from_bytes(font_data as &[u8]).expect("Error constructing Font");

        let key_w: f32 = 80.0;
        let key_h: f32 = 40.0;
        let corner_r: f32 = 8.0;
        let border_t: f32 = 2.0;
        let default_rot: f32 = 30.0;
        let txt_size: f32 = 18.0;

        let border_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x80, 0x80, 0x80);
        let background_c_default = SolidSource::from_unpremultiplied_argb(0xFF, 0xE0, 0xE0, 0xE0);
        let background_c_pressed = SolidSource::from_unpremultiplied_argb(0xFF, 0xA0, 0xA0, 0xF0);
        let text_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x10, 0x10, 0x10);

        let alt_bg = if self.left_alt_pressed { background_c_pressed } else { background_c_default };
        let ctrl_bg = if self.left_ctrl_pressed { background_c_pressed } else { background_c_default };

        let keys_to_draw = vec![
            KeyDisplay {
                text: "Alt".to_string(),
                center_x: width as f32 * 0.70, center_y: height as f32 * 0.60,
                width: key_w, height: key_h, corner_radius: corner_r, border_thickness: border_t,
                rotation_degrees: default_rot, text_size: txt_size,
                border_color: border_c, background_color: alt_bg, text_color: text_c,
            },
            KeyDisplay {
                text: "Ctrl".to_string(),
                center_x: width as f32 * 0.30, center_y: height as f32 * 0.40,
                width: key_w, height: key_h, corner_radius: corner_r, border_thickness: border_t,
                rotation_degrees: default_rot - 5.0, text_size: txt_size,
                border_color: border_c, background_color: ctrl_bg, text_color: text_c,
            },
        ];

        for key_spec in keys_to_draw {
            draw_single_key(&mut dt, &key_spec, &font);
        }

        dt.set_transform(&Transform::identity());

        let dt_buffer = dt.get_data_u8();
        for y_idx in 0..height {
            for x_idx in 0..width {
                let dt_buf_idx = (y_idx * width + x_idx) as usize * 4;
                let mmap_buf_idx = (y_idx * stride + x_idx * 4) as usize;
                if dt_buf_idx + 3 < dt_buffer.len() && mmap_buf_idx + 3 < mmap.len() {
                    let a = dt_buffer[dt_buf_idx + 3];
                    let r = dt_buffer[dt_buf_idx + 2];
                    let g = dt_buffer[dt_buf_idx + 1];
                    let b = dt_buffer[dt_buf_idx + 0];
                    let pixel_value = (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32);
                    mmap[mmap_buf_idx..mmap_buf_idx+4].copy_from_slice(&pixel_value.to_le_bytes());
                }
            }
        }

        log::info!("Drawing content");
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width, height);
        surface.commit();
        self.buffer = Some(buffer);
        self.mmap = Some(mmap);
    }
}

struct PathBuilderSink<'a>(&'a mut raqote::PathBuilder);

impl<'a> OutlineBuilder for PathBuilderSink<'a> {
    fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x, y); }
    fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x, y); }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1, y1, x, y); }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.0.cubic_to(x1, y1, x2, y2, x, y); }
    fn close(&mut self) { self.0.close(); }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
    fn event( _state: &mut AppState, proxy: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {
        if let xdg_wm_base::Event::Ping { serial } = event { proxy.pong(serial); }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
    fn event( state: &mut AppState, surface_proxy: &xdg_surface::XdgSurface, event: xdg_surface::Event, _data: &(), _conn: &Connection, qh: &QueueHandle<AppState>) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            surface_proxy.ack_configure(serial);
            if state.surface.is_some() { state.draw(qh); }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for AppState {
    fn event( state: &mut AppState, _proxy: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                log::debug!("XDG Toplevel Configure: width: {}, height: {}, states: {:?}", width, height, states);
                if width > 0 { state.configured_width = width; }
                if height > 0 { state.configured_height = height; }
            }
            xdg_toplevel::Event::Close => {
                log::info!("XDG Toplevel Close event received. Application should exit.");
                // TODO: Graceful exit
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for AppState {
    fn event( _state: &mut AppState, _proxy: &wl_compositor::WlCompositor, _event: wl_compositor::Event, _udata: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {}
}

impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event( _state: &mut AppState, _proxy: &wl_surface::WlSurface, _event: wl_surface::Event, _udata: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {}
}

impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event( _state: &mut AppState, _proxy: &wl_shm::WlShm, _event: wl_shm::Event, _udata: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {}
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event( state: &mut AppState, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _udata: &(), _conn: &Connection, qh: &QueueHandle<AppState>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => { state.compositor = Some(registry.bind::<wl_compositor::WlCompositor, (), AppState>(name, std::cmp::min(version,5), qh, ())); }
                "wl_shm" => { state.shm = Some(registry.bind::<wl_shm::WlShm, (), AppState>(name, std::cmp::min(version,1), qh, ())); }
                "xdg_wm_base" => { state.xdg_wm_base = Some(registry.bind::<xdg_wm_base::XdgWmBase, (), AppState>(name, std::cmp::min(version,3), qh, ())); }
                _ => {}
            }
        } else if let wl_registry::Event::GlobalRemove { name } = event {
            log::info!("Global removed: ID {}", name);
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppState {
    fn event( _state: &mut AppState, _proxy: &wl_shm_pool::WlShmPool, _event: wl_shm_pool::Event, _udata: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event( _state: &mut AppState, buffer: &wl_buffer::WlBuffer, event: wl_buffer::Event, _udata: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {
        if let wl_buffer::Event::Release = event { log::debug!("Buffer {:?} released", buffer.id()); }
    }
}

fn main() {
    env_logger::init();
    log::info!("Starting Wayland application...");

    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let mut app_state = AppState::new();
    let _registry = conn.display().get_registry(&qh, ());
    event_queue.roundtrip(&mut app_state).unwrap();

    if app_state.compositor.is_none() || app_state.shm.is_none() || app_state.xdg_wm_base.is_none() {
        panic!("Failed to bind essential Wayland globals.");
    }
    log::info!("Essential globals bound.");

    let interface = MyLibinputInterface;
    let mut libinput_context = input::Libinput::new_with_udev(interface);
    match libinput_context.udev_assign_seat("seat0") {
        Ok(_) => {
            log::info!("Successfully assigned seat0 to libinput context.");
            app_state.input_context = Some(libinput_context);
        }
        Err(e) => {
            log::error!("Failed to assign seat0 to libinput context: {:?}", e);
            log::error!("Input monitoring will be disabled. Ensure permissions for /dev/input devices.");
        }
    }

    let surface = app_state.compositor.as_ref().unwrap().create_surface(&qh, ());
    app_state.surface = Some(surface.clone());
    let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("Wayland Keyboard OSD".to_string());
    surface.commit();
    log::info!("Wayland window configured. Waiting for events...");

    use input::event::Event as LibinputEvent;
    use input::event::keyboard::{KeyboardEvent, KeyState, KeyboardEventTrait}; // KeyboardKeyEvent removed as it's the type of key_event
    use std::time::Duration;

    loop {
        match event_queue.dispatch_pending(&mut app_state) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error dispatching Wayland events: {}", e);
                match e {
                    wayland_client::DispatchError::Backend(err) => { // err is wayland_client::backend::WaylandError
                        match err {
                            wayland_client::backend::WaylandError::Io(io_err) => {
                                if io_err.kind() == std::io::ErrorKind::Interrupted {
                                    log::warn!("Wayland dispatch interrupted (IO), continuing.");
                                    continue;
                                }
                                log::error!("Wayland dispatch IO error: {}, breaking loop.", io_err);
                                break;
                            }
                            wayland_client::backend::WaylandError::Protocol(protocol_err) => {
                                log::error!("Wayland dispatch protocol error: {}, breaking loop.", protocol_err);
                                break;
                            }
                            // If dlopen feature is not active, Io and Protocol are exhaustive.
                            // The NoWaylandLib variant is cfg'd out.
                            // The previous _ arm caused an unreachable_patterns warning.
                        }
                    }
                    _ => {
                        log::error!("Unhandled Wayland dispatch error (not Backend): {}, breaking loop.", e);
                        break;
                    }
                }
            }
        }

        let mut needs_redraw = false;
        if let Some(ref mut context) = app_state.input_context {
            if context.dispatch().is_err() {
                log::error!("Libinput dispatch error in event processing loop");
            }
            while let Some(event) = context.next() {
                if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = event { // key_event is KeyboardKeyEvent
                    let key_code = key_event.key();
                    let key_state = key_event.key_state();
                    log::trace!("Key event: code {}, state {:?}", key_code, key_state);
                    let mut changed = false;
                    match key_code {
                        29 => {
                            let pressed = key_state == KeyState::Pressed;
                            if app_state.left_ctrl_pressed != pressed {
                                app_state.left_ctrl_pressed = pressed;
                                changed = true;
                                log::info!("Left Ctrl state changed: {}", pressed);
                            }
                        }
                        56 => {
                            let pressed = key_state == KeyState::Pressed;
                            if app_state.left_alt_pressed != pressed {
                                app_state.left_alt_pressed = pressed;
                                changed = true;
                                log::info!("Left Alt state changed: {}", pressed);
                            }
                        }
                        _ => {}
                    }
                    if changed { needs_redraw = true; }
                }
            }
        }

        if needs_redraw {
            if app_state.surface.is_some() && app_state.compositor.is_some() && app_state.shm.is_some() {
                 app_state.draw(&qh);
            } else {
                log::warn!("Skipping draw due to uninitialized Wayland components.");
            }
        }

        if let Err(e) = conn.flush() { // e is wayland_client::backend::WaylandError
            log::error!("Failed to flush Wayland connection: {}", e);
            match e {
                wayland_client::backend::WaylandError::Io(io_err) => {
                    if io_err.kind() != std::io::ErrorKind::WouldBlock {
                        log::error!("Wayland flush IO error (not WouldBlock): {}, breaking loop.", io_err);
                        break;
                    }
                    log::trace!("Wayland flush returned WouldBlock, continuing.");
                }
                _ => {
                    log::error!("Critical Wayland flush error: {:?}, breaking loop.", e);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}
