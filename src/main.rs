use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::protocol::{wl_compositor, wl_shm, wl_shm_pool, wl_surface, wl_buffer, wl_registry};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use input; // Added for libinput

use std::collections::HashMap; // For storing key states and configs by keycode
// Serde for TOML deserialization
use serde::Deserialize;
use serde_value::Value as SerdeValue; // For flexible keycode parsing
use std::fs; // For file reading
use std::process; // For exiting gracefully on config error

mod keycodes; // Our new module

// Graphics and Font rendering
use raqote::{SolidSource, PathBuilder, DrawOptions, StrokeStyle, Transform, Source};
use rusttype::{Font, Scale, point, PositionedGlyph, OutlineBuilder};
use euclid::Angle;

// Configuration Structs
#[derive(Deserialize, Debug, Clone)]
struct KeyConfig {
    name: String,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    #[serde(alias = "keycode")] // Accept "keycode" for initial deserialization
    raw_keycode: Option<SerdeValue>, // Will hold string or int from TOML, or be None
    #[serde(skip_deserializing)] // This field is populated after initial deserialization
    keycode: u32, // The resolved keycode
    rotation_degrees: Option<f32>, // Optional, defaults to 0 or a global default
    text_size: Option<f32>,       // Optional, defaults to a global default
    corner_radius: Option<f32>,   // Optional
    border_thickness: Option<f32>,// Optional
    // Colors could also be strings like "#RRGGBBAA" and parsed later
    // For now, keeping them simple, assuming they might be added if needed
    // border_color_hex: Option<String>,
    // background_color_hex: Option<String>,
    // text_color_hex: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct AppConfig {
    key: Vec<KeyConfig>,
    // Potentially global settings here, like default font, colors, etc.
    // default_text_size: Option<f32>,
    // default_corner_radius: Option<f32>,
}

// Struct to hold key properties for drawing
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
    let text_y = (key.height - key.text_size) / 2.0 + v_metrics.ascent; // This positions the baseline correctly

    // Base transformation for the entire text block (aligns it within the key)
    let base_text_transform = transform.then_translate(raqote::Vector::new(text_x, text_y));

    for glyph_instance in glyphs {
        if let Some(bounding_box) = glyph_instance.pixel_bounding_box() {
            // Create a specific transform for this glyph
            // The glyphs from font.layout are already positioned relative to the layout's origin (0, ascent_of_first_line)
            // So, we just need to apply the base_text_transform to move the whole block,
            // and then rusttype's positioning for individual glyphs takes care of the rest *if drawn relative to that point*.
            // Raqote draws paths relative to the current transform's origin.
            // build_outline builds the glyph at (0,0). So we need to translate it to its correct position.

            let glyph_specific_transform = base_text_transform
                .then_translate(raqote::Vector::new(bounding_box.min.x as f32, bounding_box.min.y as f32));

            dt.set_transform(&glyph_specific_transform);

            let mut glyph_pb = PathBuilder::new();
            // build_outline for unpositioned glyph draws it as if its origin is (0,0)
            if glyph_instance.unpositioned().build_outline(&mut PathBuilderSink(&mut glyph_pb)) {
                let glyph_path = glyph_pb.finish();
                if !glyph_path.ops.is_empty() {
                    dt.fill(&glyph_path, &Source::Solid(key.text_color), &DrawOptions::default());
                }
            }
        }
    }
    // Reset transform for subsequent drawing operations outside this function if any were planned for the same dt
    // dt.set_transform(&Transform::identity()); // Or back to `transform` if key drawing continues
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
    temp_file: Option<std::fs::File>, // Added for persistent temp file
    configured_width: i32,
    configured_height: i32,
    running: bool, // Added to control the main loop
    input_context: Option<input::Libinput>,
    // left_ctrl_pressed: bool, // Replaced by key_states
    // left_alt_pressed: bool,  // Replaced by key_states
    config: AppConfig,               // Store loaded configuration
    key_states: HashMap<u32, bool>,  // Stores pressed state for each configured keycode
    // key_config_map: HashMap<u32, KeyConfig>, // For quick lookup - might be better to build this once in new()
}

impl AppState {
    fn new(app_config: AppConfig) -> Self { // Renamed to avoid conflict
        let mut key_states_map = HashMap::new();
        for key_conf in app_config.key.iter() {
            key_states_map.insert(key_conf.keycode, false);
        }

        AppState {
            compositor: None,
            shm: None,
            xdg_wm_base: None,
            surface: None,
            buffer: None,
            mmap: None,
            temp_file: None, // Initialize temp_file
            configured_width: WINDOW_WIDTH,
            configured_height: WINDOW_HEIGHT,
            running: true, // Initialize to true
            input_context: None,
            config: app_config, // Use the passed app_config
            key_states: key_states_map, // Use the initialized map
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
        let size = (stride * height) as usize;

        // Ensure temp_file and mmap are initialized or resized if necessary
        let needs_recreation = self.temp_file.is_none() || self.mmap.is_none() ||
                               match self.mmap.as_ref() {
                                   Some(m) => m.len() < size,
                                   None => true, // Should be caught by self.mmap.is_none()
                               };

        if needs_recreation {
            if let Some(buffer) = self.buffer.take() {
                buffer.destroy();
            }
            self.mmap = None; // Drop the old mmap before the file is potentially truncated/resized
            self.temp_file = None; // Drop the old file

            let temp_f = tempfile::tempfile().expect("Failed to create temp file");
            temp_f.set_len(size as u64).expect("Failed to set temp file length");
            self.mmap = Some(unsafe { MmapMut::map_mut(&temp_f).expect("Failed to map temp file") });
            self.temp_file = Some(temp_f);
        }

        let mmap = self.mmap.as_mut().unwrap(); // Now safe to unwrap
        let temp_file_fd = self.temp_file.as_ref().unwrap().as_raw_fd();

        // Create a new pool and buffer for each draw. This is typical.
        // The expensive part was file creation/mapping, not pool/buffer creation.
        let pool = unsafe { shm.create_pool(BorrowedFd::borrow_raw(temp_file_fd), size as i32, qh, ()) };
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy(); // Pool can be destroyed after buffer creation

        let mut dt = raqote::DrawTarget::new(width, height);
        dt.clear(SolidSource::from_unpremultiplied_argb(0x00, 0x00, 0x00, 0x00));

        let scale: f32;
        let offset_x: f32;
        let offset_y: f32;

        if self.config.key.is_empty() {
            log::warn!("No keys configured. Nothing to draw.");
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, width, height);
            surface.commit();
            self.buffer = Some(buffer);
            // self.mmap = Some(mmap); // This line caused the error and is not needed here.
            return;
        } else {
            let mut min_coord_x = f32::MAX;
            let mut max_coord_x = f32::MIN;
            let mut min_coord_y = f32::MAX;
            let mut max_coord_y = f32::MIN;

            for key_config in &self.config.key {
                // Assuming key_config.x and .y are CENTER coordinates from TOML
                let key_half_width = key_config.width / 2.0;
                let key_half_height = key_config.height / 2.0;
                min_coord_x = min_coord_x.min(key_config.x - key_half_width);
                max_coord_x = max_coord_x.max(key_config.x + key_half_width);
                min_coord_y = min_coord_y.min(key_config.y - key_half_height);
                max_coord_y = max_coord_y.max(key_config.y + key_half_height);
            }

            let layout_width = max_coord_x - min_coord_x;
            let layout_height = max_coord_y - min_coord_y;

            let padding = (width.min(height) as f32 * 0.05).max(5.0); // 5% padding, min 5px
            let drawable_width = width as f32 - 2.0 * padding;
            let drawable_height = height as f32 - 2.0 * padding;

            let current_scale_x = if layout_width > 0.0 { drawable_width / layout_width } else { 1.0 };
            let current_scale_y = if layout_height > 0.0 { drawable_height / layout_height } else { 1.0 };
            scale = current_scale_x.min(current_scale_y).max(0.01); // Preserve aspect ratio, ensure scale is positive

            let scaled_layout_width = layout_width * scale;
            let scaled_layout_height = layout_height * scale;
            offset_x = padding + (drawable_width - scaled_layout_width) / 2.0 - (min_coord_x * scale);
            offset_y = padding + (drawable_height - scaled_layout_height) / 2.0 - (min_coord_y * scale);
        }

        let font_data = include_bytes!("../default-font/DejaVuSansMono.ttf");
        let font = Font::try_from_bytes(font_data as &[u8]).expect("Error constructing Font");

        // Default appearance values (unscaled)
        const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
        const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
        const DEFAULT_ROTATION_DEGREES: f32 = 0.0; // Rotation is not scaled
        const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;

        // Default colors
        let border_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x80, 0x80, 0x80);
        let background_c_default = SolidSource::from_unpremultiplied_argb(0xFF, 0xE0, 0xE0, 0xE0);
        let background_c_pressed = SolidSource::from_unpremultiplied_argb(0xFF, 0xA0, 0xA0, 0xF0);
        let text_c = SolidSource::from_unpremultiplied_argb(0xFF, 0x10, 0x10, 0x10);

        let mut keys_to_draw: Vec<KeyDisplay> = Vec::new();

        for key_config in &self.config.key {
            let is_pressed = *self.key_states.get(&key_config.keycode).unwrap_or(&false);
            let background_color = if is_pressed { background_c_pressed } else { background_c_default };

            // Apply scaling and offset
            // Original x, y from config are treated as center points
            let final_center_x = key_config.x * scale + offset_x;
            let final_center_y = key_config.y * scale + offset_y;
            let final_width = key_config.width * scale;
            let final_height = key_config.height * scale;

            let final_corner_radius = key_config.corner_radius.unwrap_or(DEFAULT_CORNER_RADIUS_UNSCALED) * scale;
            let final_border_thickness = key_config.border_thickness.unwrap_or(DEFAULT_BORDER_THICKNESS_UNSCALED) * scale;
            let final_text_size = key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) * scale;
            let final_rotation = key_config.rotation_degrees.unwrap_or(DEFAULT_ROTATION_DEGREES);


            let key_display = KeyDisplay {
                text: key_config.name.clone(),
                center_x: final_center_x,
                center_y: final_center_y,
                width: final_width,
                height: final_height,
                corner_radius: final_corner_radius,
                border_thickness: final_border_thickness,
                rotation_degrees: final_rotation, // Rotation is absolute
                text_size: final_text_size,
                border_color: border_c,
                background_color,
                text_color: text_c,
            };
            keys_to_draw.push(key_display);
        }

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

        // If a previous buffer existed, destroy it.
        // This should be handled carefully to ensure the compositor is done with it.
        // Wayland buffer release events can be used for more robust management.
        if let Some(old_buffer) = self.buffer.replace(buffer) {
            old_buffer.destroy();
        }
        // self.mmap is already updated if it was recreated.
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
                // It might be good to trigger a redraw if size changed significantly,
                // but the xdg_surface configure event usually follows and handles that.
            }
            xdg_toplevel::Event::Close => {
                log::info!("XDG Toplevel Close event received. Application will exit.");
                state.running = false; // Signal the main loop to stop
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

    // Load configuration
    let config_path = "keys.toml";
    let config_content = fs::read_to_string(config_path)
        .unwrap_or_else(|e| {
            eprintln!("Failed to read configuration file '{}': {}", config_path, e);
            process::exit(1);
        });

    let mut app_config: AppConfig = toml::from_str(&config_content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse TOML configuration from '{}': {}", config_path, e);
            process::exit(1);
        });

    // Process raw_keycode to populate keycode
    for key_conf in app_config.key.iter_mut() {
        let resolved_code = match key_conf.raw_keycode.as_ref() {
            Some(SerdeValue::String(s)) => {
                keycodes::get_keycode_from_string(s)
            }
            // Handle various integer types that serde_value might produce from TOML
            Some(SerdeValue::U8(i)) => Ok(*i as u32),
            Some(SerdeValue::U16(i)) => Ok(*i as u32),
            Some(SerdeValue::U32(i)) => Ok(*i),
            Some(SerdeValue::U64(i)) => {
                if *i <= u32::MAX as u64 {
                    Ok(*i as u32)
                } else {
                    Err(format!("Integer keycode {} for key '{}' is too large for u32.", i, key_conf.name))
                }
            }
            Some(SerdeValue::I8(i)) => {
                if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
            }
            Some(SerdeValue::I16(i)) => {
                if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
            }
            Some(SerdeValue::I32(i)) => {
                if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
            }
            Some(SerdeValue::I64(i)) => {
                if *i >= 0 && *i <= u32::MAX as i64 {
                    Ok(*i as u32)
                } else {
                    Err(format!("Integer keycode {} for key '{}' is out of valid u32 range.", i, key_conf.name))
                }
            }
            None => { // Default to name field
                keycodes::get_keycode_from_string(&key_conf.name)
            }
            Some(other_type) => {
                 Err(format!("Invalid type for keycode field for key '{}': expected string or integer, got {:?}", key_conf.name, other_type))
            }
        };

        match resolved_code {
            Ok(code) => key_conf.keycode = code,
            Err(e) => {
                // If defaulting from 'name' failed, guide user to set 'keycode'
                if key_conf.raw_keycode.is_none() {
                     eprintln!(
                        "Error processing key '{}': Could not resolve default keycode from name ('{}'). Please specify a 'keycode' field for this key. Details: {}",
                        key_conf.name, key_conf.name, e
                    );
                } else {
                    eprintln!("Error processing keycode for key '{}': {}", key_conf.name, e);
                }
                process::exit(1);
            }
        }
    }

    log::info!("Configuration loaded and processed: {:?}", app_config);

    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let mut app_state = AppState::new(app_config.clone()); // Pass processed config to AppState
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

    // Dispatch events once to process initial configure and draw the window.
    log::info!("Initial surface commit done. Dispatching events to catch initial configure...");
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during initial roundtrip after surface commit.");
        // Depending on the error, might want to exit or handle differently
    }
    log::info!("Initial roundtrip complete. Wayland window should be configured and drawn. Waiting for further events...");

    use input::event::Event as LibinputEvent;
    use input::event::keyboard::{KeyboardEvent, KeyState, KeyboardEventTrait};
    // use std::time::Duration; // No longer needed
    use wayland_client::backend::WaylandError;
    use std::os::unix::io::{AsRawFd as _, RawFd};
    // use std::ptr; // No longer needed if pollfd is zeroed carefully or fully initialized
    use std::io; // For io::Error

    log::info!("Entering main event loop.");

    let wayland_raw_fd: RawFd = conn.prepare_read().unwrap().connection_fd().as_raw_fd();
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
    }

    // Timeout for poll in milliseconds
    let poll_timeout_ms = 100;

    while app_state.running {
        // Reset revents before each poll call
        for item in fds.iter_mut() {
            item.revents = 0;
        }
        let mut needs_redraw = false; // Initialize needs_redraw at the start of the loop iteration

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, poll_timeout_ms) };

        if ret < 0 {
            let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::EINTR {
                log::trace!("libc::poll interrupted, continuing.");
                continue;
            }
            log::error!("libc::poll error: {}", io::Error::last_os_error());
            app_state.running = false;
            break;
        } else if ret == 0 {
            // Timeout, no events
            log::trace!("libc::poll timeout, no events.");
        } else {
            // Events are ready

            // Check Wayland FD
            if (fds[WAYLAND_FD_IDX].revents & libc::POLLIN) != 0 {
                log::trace!("Wayland FD readable (POLLIN)");
                if let Some(guard) = conn.prepare_read() {
                    match guard.read() {
                        Ok(n) => {
                            log::trace!("Successfully read {} bytes from Wayland socket (after poll)", n);
                            // Dispatch pending Wayland events. Non-blocking.
                            match event_queue.dispatch_pending(&mut app_state) {
                                Ok(_) => { log::trace!("Wayland events dispatched successfully."); }
                                Err(e) => {
                                    log::error!("Error in dispatch_pending: {}", e);
                                    // Handle Wayland dispatch errors similarly to before
                                    match e {
                                        wayland_client::DispatchError::Backend(WaylandError::Io(io_err)) => {
                                            if io_err.kind() == std::io::ErrorKind::Interrupted {
                                                log::warn!("Wayland dispatch_pending interrupted (IO), continuing.");
                                            } else {
                                                log::error!("Wayland dispatch_pending IO error: {}, exiting.", io_err);
                                                app_state.running = false;
                                            }
                                        }
                                        wayland_client::DispatchError::Backend(WaylandError::Protocol(protocol_err)) => {
                                            log::error!("Wayland dispatch_pending protocol error: {}, exiting.", protocol_err);
                                            app_state.running = false;
                                        }
                                        _ => {
                                            log::error!("Unhandled Wayland dispatch_pending error: {}, exiting.", e);
                                            app_state.running = false;
                                        }
                                    }
                                }
                            }
                        }
                        Err(WaylandError::Io(io_err)) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
                            log::trace!("Wayland read would block, no new events this cycle (after poll).");
                        }
                        Err(WaylandError::Io(io_err)) => {
                            log::error!("Error reading from Wayland connection (after poll): {}", io_err);
                            app_state.running = false;
                        }
                        Err(e) => {
                            log::error!("Error reading from Wayland connection (non-IO, after poll): {}", e);
                            app_state.running = false;
                        }
                    }
                } else {
                    log::warn!("Failed to prepare_read on Wayland connection after poll.");
                }
            }
            if (fds[WAYLAND_FD_IDX].revents & libc::POLLERR) != 0 || (fds[WAYLAND_FD_IDX].revents & libc::POLLHUP) != 0 {
                 log::error!("Wayland FD error/hangup (POLLERR/POLLHUP). Exiting.");
                 app_state.running = false;
            }


            // Check libinput FD
            if let Some(libinput_idx) = libinput_fd_idx_opt {
                if app_state.running && (fds[libinput_idx].revents & libc::POLLIN) != 0 {
                    log::trace!("Libinput FD readable (POLLIN)");
                    if let Some(ref mut context) = app_state.input_context {
                        if context.dispatch().is_err() {
                            log::error!("Libinput dispatch error in event processing loop");
                        }
                        while let Some(libinput_event) = context.next() {
                            if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = libinput_event {
                                let key_code = key_event.key();
                                let key_state = key_event.key_state();
                                log::trace!("Key event: code {}, state {:?}", key_code, key_state);

                                let pressed = key_state == KeyState::Pressed;
                                if let Some(current_state) = app_state.key_states.get_mut(&key_code) {
                                    if *current_state != pressed {
                                        *current_state = pressed;
                                        needs_redraw = true;
                                        log::info!("Configured key {} state changed: {}", key_code, pressed);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !app_state.running {
            break;
        }

        if needs_redraw {
            if app_state.surface.is_some() && app_state.compositor.is_some() && app_state.shm.is_some() {
                 app_state.draw(&qh);
            } else {
                log::warn!("Skipping draw due to uninitialized Wayland components.");
            }
        }

        match conn.flush() {
            Ok(_) => { log::trace!("Wayland connection flushed successfully."); }
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                log::warn!("Wayland flush would block. Messages might be delayed.");
            }
            Err(e) => {
                log::error!("Failed to flush Wayland connection: {}", e);
                app_state.running = false;
            }
        }

        // The main loop using libc::poll doesn't require explicit re-registration of FDs in this manner.
        // The fds array is re-used in each call to libc::poll.
    }
    log::info!("Exiting application loop.");
}
