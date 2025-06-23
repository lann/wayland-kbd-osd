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
use clap::Parser; // For command-line argument parsing

mod keycodes; // Our new module

// Graphics and Font rendering
// use raqote::{SolidSource, PathBuilder, DrawOptions, StrokeStyle, Transform, Source}; // Replaced by cairo
// use rusttype::{Font, Scale, point, PositionedGlyph, OutlineBuilder}; // Replaced by cairo
use cairo::{Context, ImageSurface, Format, FontFace as CairoFontFace}; // Added for cairo-rs (Removed FtFontFace, FontSlant, FontWeight, Surface)
use freetype::{Library as FreeTypeLibrary}; // Added for freetype-rs, removed unused FreeTypeLoadFlag
// use euclid::Angle; // No longer needed

// Configuration Structs
#[derive(Deserialize, Debug, Clone)]
struct KeyConfig {
    name: String,
    width: f32,
    height: f32,
    left: f32,
    top: f32,
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
    // Colors are now (R, G, B, A) tuples with values from 0.0 to 1.0
    border_color: (f64, f64, f64, f64),
    background_color: (f64, f64, f64, f64),
    text_color: (f64, f64, f64, f64),
}

// fn draw_single_key( // This function will be rewritten for Cairo
//     dt: &mut raqote::DrawTarget,
//     key: &KeyDisplay,
//     font: &Font<'_>
// ) {
    // ... old raqote implementation ...
// }

// New function using Cairo
fn draw_single_key_cairo(
    ctx: &Context,
    key: &KeyDisplay,
) {
    let x = key.center_x as f64;
    let y = key.center_y as f64;
    let width = key.width as f64;
    let height = key.height as f64;
    let corner_radius = key.corner_radius as f64;
    let border_thickness = key.border_thickness as f64;
    let rotation_radians = key.rotation_degrees.to_radians() as f64;

    ctx.save().expect("Failed to save cairo context state");

    // Set up transformation: translate to key center, rotate, then translate to top-left of key bounding box
    ctx.translate(x, y);
    ctx.rotate(rotation_radians);
    ctx.translate(-width / 2.0, -height / 2.0);

    // Draw rounded rectangle path
    // Cairo's `arc` is clockwise. `arc_negative` is counter-clockwise.
    // Angles are in radians. 0 is east, PI/2 is south, PI is west, 3*PI/2 is north.
    ctx.new_sub_path();
    ctx.arc(width - corner_radius, corner_radius, corner_radius, -std::f64::consts::PI / 2.0, 0.0); // Top-right corner
    ctx.arc(width - corner_radius, height - corner_radius, corner_radius, 0.0, std::f64::consts::PI / 2.0); // Bottom-right corner
    ctx.arc(corner_radius, height - corner_radius, corner_radius, std::f64::consts::PI / 2.0, std::f64::consts::PI); // Bottom-left corner
    ctx.arc(corner_radius, corner_radius, corner_radius, std::f64::consts::PI, 3.0 * std::f64::consts::PI / 2.0); // Top-left corner
    ctx.close_path();

    // Fill
    let (r, g, b, a) = key.background_color;
    ctx.set_source_rgba(r, g, b, a);
    ctx.fill_preserve().expect("Cairo fill failed"); // Use fill_preserve to keep path for stroke

    // Stroke
    let (r, g, b, a) = key.border_color;
    ctx.set_source_rgba(r, g, b, a);
    ctx.set_line_width(border_thickness);
    ctx.stroke().expect("Cairo stroke failed");

    // Text drawing
    let (r, g, b, a) = key.text_color;
    ctx.set_source_rgba(r, g, b, a);

    // --- Text Scaling and Truncation Logic ---
    let mut current_text = key.text.clone();
    let mut current_font_size = key.text_size as f64;
    ctx.set_font_size(current_font_size); // Initial font size

    // Define text area constraints
    let text_padding = (key.width * 0.1).min(key.height * 0.1).max(2.0) as f64; // 10% padding, min 2px
    let max_text_width = width - 2.0 * text_padding;
    // let max_text_height = height - 2.0 * text_padding; // Max height can also be a constraint

    let original_font_size = key.text_size as f64;
    let min_font_size = (original_font_size * 0.5).max(6.0); // Min 50% of original, or 6.0 points

    let mut text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (initial)");

    // 1. Font size scaling
    while text_extents.width() > max_text_width && current_font_size > min_font_size {
        current_font_size *= 0.9; // Reduce font size by 10%
        if current_font_size < min_font_size {
            current_font_size = min_font_size;
        }
        ctx.set_font_size(current_font_size);
        text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (scaling)");
        if current_font_size == min_font_size && text_extents.width() > max_text_width {
            break; // Reached min font size, proceed to truncation if still too wide
        }
    }

    // 2. Text truncation
    if text_extents.width() > max_text_width {
        let ellipsis = "...";
        let ellipsis_extents = ctx.text_extents(ellipsis).expect("Failed to get ellipsis extents");
        let max_width_for_text_with_ellipsis = max_text_width - ellipsis_extents.width();

        while text_extents.width() > max_text_width && !current_text.is_empty() {
            if current_text.pop().is_none() { // Remove last char
                break; // Should not happen if !current_text.is_empty()
            }
            let temp_text_with_ellipsis = if current_text.is_empty() {
                // If all text removed, maybe just show ellipsis if it fits, or nothing
                if ellipsis_extents.width() <= max_text_width { ellipsis.to_string() } else { "".to_string() }
            } else {
                format!("{}{}", current_text, ellipsis)
            };

            text_extents = ctx.text_extents(&temp_text_with_ellipsis).expect("Failed to get text extents (truncating)");

            // Check if the current_text part (before adding ellipsis) is too long
            let current_text_only_extents = ctx.text_extents(&current_text).expect("Failed to get current_text extents");

            if current_text_only_extents.width() <= max_width_for_text_with_ellipsis || current_text.is_empty() {
                 current_text = temp_text_with_ellipsis;
                 text_extents = ctx.text_extents(&current_text).expect("Failed to get final truncated text extents");
                 break;
            }
        }
         // Final check, if even ellipsis doesn't fit, make text empty or just one/two chars of ellipsis
        if text_extents.width() > max_text_width {
            if ellipsis_extents.width() <= max_text_width {
                current_text = ellipsis.to_string();
                if current_text.len() > 1 && ctx.text_extents("..").unwrap().width() <= max_text_width {
                    current_text = "..".to_string();
                } else if current_text.len() > 0 && ctx.text_extents(".").unwrap().width() <= max_text_width {
                     current_text = ".".to_string();
                } else {
                    current_text = "".to_string(); // Nothing fits
                }
            } else {
                 current_text = "".to_string(); // Ellipsis itself is too wide
            }
            // text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (final truncation check)"); // This was redundant
        }
    }
    // --- End of Text Scaling and Truncation ---

    // Recalculate text_extents with final text and font size
    // ctx.set_font_size(current_font_size); // Already set during scaling/truncation
    let text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (final)");


    // Calculate text position to center it
    let text_x = (width - text_extents.width()) / 2.0 - text_extents.x_bearing();
    let text_y = (height - text_extents.height()) / 2.0 - text_extents.y_bearing();

    ctx.move_to(text_x, text_y);
    ctx.show_text(&current_text).expect("Cairo show_text failed");

    ctx.restore().expect("Failed to restore cairo context state");
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

        // self.mmap is guaranteed to be Some by the logic above.
        // let mmap = self.mmap.as_mut().unwrap(); // This variable was unused, mmap_data below is used.
        let temp_file_fd = self.temp_file.as_ref().unwrap().as_raw_fd();

        // Create a new pool and buffer for each draw. This is typical.
        let pool = unsafe { shm.create_pool(BorrowedFd::borrow_raw(temp_file_fd), size as i32, qh, ()) };
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy(); // Pool can be destroyed after buffer creation

        // Get a mutable pointer to the mmap data for Cairo.
        // This is unsafe because we are responsible for ensuring the data outlives the surface.
        // In this case, the surface (`cairo_surface`) is local to this `draw` method,
        // and `self.mmap` (the source of the data) outlives this method.
        let mmap_ptr = self.mmap.as_mut().unwrap().as_mut_ptr();

        let cairo_surface = match unsafe {
            ImageSurface::create_for_data_unsafe(
                mmap_ptr,           // Raw pointer to the data
                Format::ARgb32,     // Corresponds to wl_shm::Format::Argb8888
                width,
                height,
                stride,
            )
        } {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create Cairo ImageSurface from mmap data (unsafe): {:?}", e);
                surface.attach(Some(&buffer), 0, 0);
                surface.damage_buffer(0, 0, width, height);
                surface.commit();
                if let Some(old_buffer) = self.buffer.replace(buffer) {
                    old_buffer.destroy();
                }
                return;
            }
        };

        let ctx = Context::new(&cairo_surface).expect("Failed to create Cairo Context");

        // Clear the surface (transparent black)
        ctx.save().unwrap(); // Save context state before changing operator
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.0); // Transparent
        ctx.set_operator(cairo::Operator::Source); // Replace content
        ctx.paint().expect("Cairo paint (clear) failed");
        ctx.restore().unwrap(); // Restore operator and other states

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
                // key_config.left and .top are TOP-LEFT coordinates from TOML
                min_coord_x = min_coord_x.min(key_config.left);
                max_coord_x = max_coord_x.max(key_config.left + key_config.width);
                min_coord_y = min_coord_y.min(key_config.top);
                max_coord_y = max_coord_y.max(key_config.top + key_config.height);
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

        let font_data: &[u8] = include_bytes!("../default-font/DejaVuSansMono.ttf");
        let ft_library = FreeTypeLibrary::init().expect("Failed to init FreeType library");
        let ft_face = ft_library.new_memory_face(font_data.to_vec(), 0).expect("Failed to load font from memory");
        // We might want to set char size here on ft_face if needed, e.g., ft_face.set_pixel_sizes(0, 32)
        // However, Cairo's context.set_font_size() is usually preferred.
        // Note: create_from_ft might require specific freetype features enabled in cairo-rs or specific versions.
        // If this exact method name is wrong for cairo-rs 0.19 + freetype-rs 0.35, it will need adjustment.
        let cairo_font_face = CairoFontFace::create_from_ft(&ft_face)
            .expect("Failed to create Cairo font face from FT face");

        // Default appearance values (unscaled)
        // const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0; // Now a global const
        const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
        const DEFAULT_ROTATION_DEGREES: f32 = 0.0; // Rotation is not scaled
        // const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0; // Now a global const

        // Default colors for Cairo: (R, G, B, A) with values from 0.0 to 1.0
        let border_c_cairo = (0x80 as f64 / 255.0, 0x80 as f64 / 255.0, 0x80 as f64 / 255.0, 0xFF as f64 / 255.0);
        let background_c_default_cairo = (0xE0 as f64 / 255.0, 0xE0 as f64 / 255.0, 0xE0 as f64 / 255.0, 0xFF as f64 / 255.0);
        let background_c_pressed_cairo = (0xA0 as f64 / 255.0, 0xA0 as f64 / 255.0, 0xF0 as f64 / 255.0, 0xFF as f64 / 255.0);
        let text_c_cairo = (0x10 as f64 / 255.0, 0x10 as f64 / 255.0, 0x10 as f64 / 255.0, 0xFF as f64 / 255.0);


        let mut keys_to_draw: Vec<KeyDisplay> = Vec::new();

        for key_config in &self.config.key {
            let is_pressed = *self.key_states.get(&key_config.keycode).unwrap_or(&false);
            let background_color = if is_pressed { background_c_pressed_cairo } else { background_c_default_cairo };

            // Apply scaling and offset
            // key_config.left and .top are top-left coordinates.
            // We need to calculate the center for drawing.
            let final_center_x = (key_config.left + key_config.width / 2.0) * scale + offset_x;
            let final_center_y = (key_config.top + key_config.height / 2.0) * scale + offset_y;
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
                border_color: border_c_cairo,
                background_color, // This is already background_c_pressed_cairo or background_c_default_cairo
                text_color: text_c_cairo,
            };
            keys_to_draw.push(key_display);
        }

        // Set the font face once on the context (assuming it's the same for all keys)
        ctx.set_font_face(&cairo_font_face);

        for key_spec in keys_to_draw {
            // Set font size for each key specifically, as it can vary
            ctx.set_font_size(key_spec.text_size as f64);
            draw_single_key_cairo(&ctx, &key_spec);
        }

        // Ensure all drawing operations are written to the underlying buffer.
        // For an ImageSurface created with create_for_data, operations are generally direct.
        // However, calling flush can be a good practice to ensure completion.
        cairo_surface.flush();

        // The manual pixel copy loop is no longer needed as Cairo draws directly into mmap_slice.
        // The mmap_slice was a mutable borrow from self.mmap.as_mut().unwrap(), so changes are reflected.

        log::info!("Drawing content with Cairo complete.");
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

// Default appearance values (unscaled) - also used in check_config
const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
// const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0; // Defined in draw()
const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;
// const DEFAULT_ROTATION_DEGREES: f32 = 0.0; // Defined in draw()


// struct PathBuilderSink<'a>(&'a mut raqote::PathBuilder); // Removed, was for raqote/rusttype

// impl<'a> OutlineBuilder for PathBuilderSink<'a> { // Removed
//     fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x, y); }
//     fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x, y); }
//     fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1, y1, x, y); }
//     fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.0.cubic_to(x1, y1, x2, y2, x, y); }
//     fn close(&mut self) { self.0.close(); }
// }

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

/* PREVIOUS MAIN FUNCTION - COMMENTED OUT
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
*/


/// Command-line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Check the configuration file for errors and print layout information
    #[clap(long)]
    check: bool,

    /// Path to the configuration file
    #[clap(long, value_parser, default_value = "keys.toml")]
    config: String,
}

// Helper function for --check: Validate configuration
fn validate_config(config: &AppConfig) -> Result<(), String> {
    // Check for overlapping keys
    for i in 0..config.key.len() {
        for j in (i + 1)..config.key.len() {
            let key1 = &config.key[i];
            let key2 = &config.key[j];

            // Basic bounding box check (ignoring rotation for simplicity in this check)
            let k1_left = key1.left;
            let k1_right = key1.left + key1.width;
            let k1_top = key1.top;
            let k1_bottom = key1.top + key1.height;

            let k2_left = key2.left;
            let k2_right = key2.left + key2.width;
            let k2_top = key2.top;
            let k2_bottom = key2.top + key2.height;

            if k1_left < k2_right && k1_right > k2_left && k1_top < k2_bottom && k1_bottom > k2_top {
                return Err(format!(
                    "Configuration validation error: Key '{}' (at {:.1},{:.1} size {:.1}x{:.1}) overlaps with key '{}' (at {:.1},{:.1} size {:.1}x{:.1})",
                    key1.name, key1.left, key1.top, key1.width, key1.height,
                    key2.name, key2.left, key2.top, key2.width, key2.height
                ));
            }
        }
    }

    // Check for duplicate keycodes
    let mut keycodes_seen = HashMap::new();
    for key_config in &config.key {
        if let Some(existing_key_name) = keycodes_seen.get(&key_config.keycode) {
            return Err(format!(
                "Configuration validation error: Duplicate keycode {} detected. Used by key '{}' and key '{}'.",
                key_config.keycode, existing_key_name, key_config.name
            ));
        }
        keycodes_seen.insert(key_config.keycode, key_config.name.clone());
    }

    // Check for invalid values (e.g. negative width/height)
    for key_config in &config.key {
        if key_config.width <= 0.0 {
            return Err(format!("Configuration validation error: Key '{}' has non-positive width {:.1}.", key_config.name, key_config.width));
        }
        if key_config.height <= 0.0 {
            return Err(format!("Configuration validation error: Key '{}' has non-positive height {:.1}.", key_config.name, key_config.height));
        }
        // text_size is optional, but if present, should be positive
        if let Some(ts) = key_config.text_size {
            if ts <= 0.0 {
                 return Err(format!("Configuration validation error: Key '{}' has non-positive text_size {:.1}.", key_config.name, ts));
            }
        }
         // corner_radius is optional, but if present, should be non-negative
        if let Some(cr) = key_config.corner_radius {
            if cr < 0.0 {
                 return Err(format!("Configuration validation error: Key '{}' has negative corner_radius {:.1}.", key_config.name, cr));
            }
        }
         // border_thickness is optional, but if present, should be non-negative
        if let Some(bt) = key_config.border_thickness {
            if bt < 0.0 {
                 return Err(format!("Configuration validation error: Key '{}' has negative border_thickness {:.1}.", key_config.name, bt));
            }
        }
    }


    Ok(())
}

// Helper struct for --check: Text metrics simulation result
struct TextCheckResult {
    final_font_size_pts: f64,
    truncated_chars: usize,
    final_text: String,
}

// Helper function for --check: Simulate text scaling and truncation
fn simulate_text_layout(
    key_config: &KeyConfig,
    ft_face: &freetype::Face, // Pass FreeType face for metrics
) -> Result<TextCheckResult, String> {
    let original_text = key_config.name.clone();
    let key_width = key_config.width as f64; // Use f64 for consistency with Cairo/FreeType
    let key_height = key_config.height as f64;

    let original_font_size_pts = key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64;

    // Define text area constraints (similar to draw_single_key_cairo)
    // This padding is applied to the *unscaled* key dimensions for the check.
    let text_padding = (key_width * 0.1).min(key_height * 0.1).max(2.0);
    let max_text_width_px = key_width - 2.0 * text_padding;
    // let max_text_height_px = key_height - 2.0 * text_padding; // Max height can also be a constraint

    let min_font_size_pts = (original_font_size_pts * 0.5).max(6.0);

    let mut current_text = original_text.clone();
    let mut current_font_size_pts = original_font_size_pts;
    let mut truncated_chars = 0;

    // Function to get text width using FreeType
    // Note: FreeType's pixel sizes are typically integer, but set_char_size can take 26.6 fixed point.
    // For simplicity here, we'll round to nearest pixel for set_pixel_sizes.
    // A more accurate simulation might use set_char_size with fractional points.
    let get_ft_text_width = |text: &str, size_pts: f64, face: &freetype::Face| -> Result<f64, String> {
        // Convert points to pixels for FreeType (assuming 96 DPI, standard for many systems)
        // Pts to Px: Px = Pt * DPI / 72
        // However, freetype's set_pixel_sizes is more direct if we assume pts = px for this simulation
        // Or, if text_size in TOML is meant as pixel height:
        let pixel_height = size_pts.round() as u32;
        if pixel_height == 0 { return Ok(0.0); } // Avoid error with zero size

        face.set_pixel_sizes(0, pixel_height).map_err(|e| format!("FreeType set_pixel_sizes failed: {:?}", e))?;

        let mut total_width = 0.0;
        for char_code in text.chars() {
            face.load_char(char_code as usize, freetype::face::LoadFlag::RENDER)
                .map_err(|e| format!("FreeType load_char failed for '{}': {:?}", char_code, e))?;
            total_width += face.glyph().advance().x as f64 / 64.0; // Advance is in 1/64th of a pixel
        }
        Ok(total_width)
    };

    let mut text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;

    // 1. Font size scaling
    while text_width_px > max_text_width_px && current_font_size_pts > min_font_size_pts {
        current_font_size_pts *= 0.9;
        if current_font_size_pts < min_font_size_pts {
            current_font_size_pts = min_font_size_pts;
        }
        text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
        if current_font_size_pts == min_font_size_pts && text_width_px > max_text_width_px {
            break;
        }
    }

    // 2. Text truncation
    if text_width_px > max_text_width_px {
        let ellipsis = "...";
        let ellipsis_width_px = get_ft_text_width(ellipsis, current_font_size_pts, ft_face)?;
        // let max_width_for_text_with_ellipsis = max_text_width_px - ellipsis_width_px;

        while text_width_px > max_text_width_px && !current_text.is_empty() {
            // let original_len = current_text.chars().count(); // This was unused
            let initial_len_before_pop = current_text.chars().count();
            current_text.pop(); // Remove last char
            // Correctly calculate truncated_chars based on original text length and current length after pop
            truncated_chars = original_text.chars().count() - current_text.chars().count();
            if current_text.chars().count() < initial_len_before_pop { // A char was actually popped
                 // This logic was slightly off, already handled by `original_text.chars().count() - current_text.chars().count()`
            }

            if current_text.is_empty() {
                current_text = if ellipsis_width_px <= max_text_width_px { ellipsis.to_string() } else { "".to_string() };
                text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
                break;
            }

            let temp_text_with_ellipsis = format!("{}{}", current_text, ellipsis);
            text_width_px = get_ft_text_width(&temp_text_with_ellipsis, current_font_size_pts, ft_face)?;

            // More robust truncation: check if current_text + ellipsis fits.
            // If current_text itself (without ellipsis) is already too small for max_width_for_text_with_ellipsis,
            // then we must have added the ellipsis.
            let current_text_only_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
            if current_text_only_width_px + ellipsis_width_px <= max_text_width_px {
                 current_text = temp_text_with_ellipsis;
                 text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
                 break;
            }
        }

        // Final check, if even ellipsis doesn't fit
        if text_width_px > max_text_width_px {
             let mut temp_ellipsis = ellipsis.to_string();
             while get_ft_text_width(&temp_ellipsis, current_font_size_pts, ft_face)? > max_text_width_px && !temp_ellipsis.is_empty() {
                temp_ellipsis.pop();
             }
             current_text = temp_ellipsis;
             // Update truncated_chars based on how much of original_text is left vs how much of ellipsis is shown
             // This is a bit tricky. If current_text is now ".." or ".", it means original was fully truncated.
             if current_text.starts_with(ellipsis.chars().next().unwrap_or_default()) && current_text.len() < ellipsis.len() {
                truncated_chars = original_text.chars().count();
             } else if current_text.is_empty() {
                truncated_chars = original_text.chars().count();
             }
        }
    }

    Ok(TextCheckResult {
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
        final_text: current_text,
    })
}


// fn main() { // Original main function, to be removed or commented out
//     env_logger::init();
//     log::info!("Starting Wayland application...");
//
//     // Load configuration
//     let config_path = "keys.toml";
//     let config_content = fs::read_to_string(config_path)
//         .unwrap_or_else(|e| {
//             eprintln!("Failed to read configuration file '{}': {}", config_path, e);
//             process::exit(1);
//         });
//
//     let mut app_config: AppConfig = toml::from_str(&config_content)
//         .unwrap_or_else(|e| {
//             eprintln!("Failed to parse TOML configuration from '{}': {}", config_path, e);
//             process::exit(1);
//         });
//
//     // Process raw_keycode to populate keycode
//     for key_conf in app_config.key.iter_mut() {
//         let resolved_code = match key_conf.raw_keycode.as_ref() {
//             Some(SerdeValue::String(s)) => {
//                 keycodes::get_keycode_from_string(s)
//             }
//             // Handle various integer types that serde_value might produce from TOML
//             Some(SerdeValue::U8(i)) => Ok(*i as u32),
//             Some(SerdeValue::U16(i)) => Ok(*i as u32),
//             Some(SerdeValue::U32(i)) => Ok(*i),
//             Some(SerdeValue::U64(i)) => {
//                 if *i <= u32::MAX as u64 {
//                     Ok(*i as u32)
//                 } else {
//                     Err(format!("Integer keycode {} for key '{}' is too large for u32.", i, key_conf.name))
//                 }
//             }
//             Some(SerdeValue::I8(i)) => {
//                 if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
//             }
//             Some(SerdeValue::I16(i)) => {
//                 if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
//             }
//             Some(SerdeValue::I32(i)) => {
//                 if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) }
//             }
//             Some(SerdeValue::I64(i)) => {
//                 if *i >= 0 && *i <= u32::MAX as i64 {
//                     Ok(*i as u32)
//                 } else {
//                     Err(format!("Integer keycode {} for key '{}' is out of valid u32 range.", i, key_conf.name))
//                 }
//             }
//             None => { // Default to name field
//                 keycodes::get_keycode_from_string(&key_conf.name)
//             }
//             Some(other_type) => {
//                  Err(format!("Invalid type for keycode field for key '{}': expected string or integer, got {:?}", key_conf.name, other_type))
//             }
//         };
//
//         match resolved_code {
//             Ok(code) => key_conf.keycode = code,
//             Err(e) => {
//                 // If defaulting from 'name' failed, guide user to set 'keycode'
//                 if key_conf.raw_keycode.is_none() {
//                      eprintln!(
//                         "Error processing key '{}': Could not resolve default keycode from name ('{}'). Please specify a 'keycode' field for this key. Details: {}",
//                         key_conf.name, key_conf.name, e
//                     );
//                 } else {
//                     eprintln!("Error processing keycode for key '{}': {}", key_conf.name, e);
//                 }
//                 process::exit(1);
//             }
//         }
//     }
//
//     log::info!("Configuration loaded and processed: {:?}", app_config);
//
//     let conn = Connection::connect_to_env().unwrap();
//     let mut event_queue = conn.new_event_queue();
//     let qh = event_queue.handle();
//     let mut app_state = AppState::new(app_config.clone()); // Pass processed config to AppState
//     let _registry = conn.display().get_registry(&qh, ());
//     event_queue.roundtrip(&mut app_state).unwrap();
//
//     if app_state.compositor.is_none() || app_state.shm.is_none() || app_state.xdg_wm_base.is_none() {
//         panic!("Failed to bind essential Wayland globals.");
//     }
//     log::info!("Essential globals bound.");
//
//     let interface = MyLibinputInterface;
//     let mut libinput_context = input::Libinput::new_with_udev(interface);
//     match libinput_context.udev_assign_seat("seat0") {
//         Ok(_) => {
//             log::info!("Successfully assigned seat0 to libinput context.");
//             app_state.input_context = Some(libinput_context);
//         }
//         Err(e) => {
//             log::error!("Failed to assign seat0 to libinput context: {:?}", e);
//             log::error!("Input monitoring will be disabled. Ensure permissions for /dev/input devices.");
//         }
//     }
//
//     let surface = app_state.compositor.as_ref().unwrap().create_surface(&qh, ());
//     app_state.surface = Some(surface.clone());
//     let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
//     let toplevel = xdg_surface.get_toplevel(&qh, ());
//     toplevel.set_title("Wayland Keyboard OSD".to_string());
//     surface.commit();
//
//     // Dispatch events once to process initial configure and draw the window.
//     log::info!("Initial surface commit done. Dispatching events to catch initial configure...");
//     if event_queue.roundtrip(&mut app_state).is_err() {
//         log::error!("Error during initial roundtrip after surface commit.");
//         // Depending on the error, might want to exit or handle differently
//     }
//     log::info!("Initial roundtrip complete. Wayland window should be configured and drawn. Waiting for further events...");
//
//     use input::event::Event as LibinputEvent;
//     use input::event::keyboard::{KeyboardEvent, KeyState, KeyboardEventTrait};
//     // use std::time::Duration; // No longer needed
//     use wayland_client::backend::WaylandError;
//     use std::os::unix::io::{AsRawFd as _, RawFd};
//     // use std::ptr; // No longer needed if pollfd is zeroed carefully or fully initialized
//     use std::io; // For io::Error
//
//     log::info!("Entering main event loop.");
//
//     let wayland_raw_fd: RawFd = conn.prepare_read().unwrap().connection_fd().as_raw_fd();
//     let mut fds: Vec<libc::pollfd> = Vec::new();
//
//     fds.push(libc::pollfd {
//         fd: wayland_raw_fd,
//         events: libc::POLLIN,
//         revents: 0,
//     });
//     const WAYLAND_FD_IDX: usize = 0;
//
//     let mut libinput_fd_idx_opt: Option<usize> = None;
//     if let Some(ref context) = app_state.input_context {
//         let libinput_raw_fd: RawFd = context.as_raw_fd();
//         fds.push(libc::pollfd {
//             fd: libinput_raw_fd,
//             events: libc::POLLIN,
//             revents: 0,
//         });
//         libinput_fd_idx_opt = Some(fds.len() - 1);
//     }
//
//     // Timeout for poll in milliseconds
//     let poll_timeout_ms = 100;
//
//     while app_state.running {
//         // Reset revents before each poll call
//         for item in fds.iter_mut() {
//             item.revents = 0;
//         }
//         let mut needs_redraw = false; // Initialize needs_redraw at the start of the loop iteration
//
//         let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, poll_timeout_ms) };
//
//         if ret < 0 {
//             let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
//             if errno == libc::EINTR {
//                 log::trace!("libc::poll interrupted, continuing.");
//                 continue;
//             }
//             log::error!("libc::poll error: {}", io::Error::last_os_error());
//             app_state.running = false;
//             break;
//         } else if ret == 0 {
//             // Timeout, no events
//             log::trace!("libc::poll timeout, no events.");
//         } else {
//             // Events are ready
//
//             // Check Wayland FD
//             if (fds[WAYLAND_FD_IDX].revents & libc::POLLIN) != 0 {
//                 log::trace!("Wayland FD readable (POLLIN)");
//                 if let Some(guard) = conn.prepare_read() {
//                     match guard.read() {
//                         Ok(n) => {
//                             log::trace!("Successfully read {} bytes from Wayland socket (after poll)", n);
//                             // Dispatch pending Wayland events. Non-blocking.
//                             match event_queue.dispatch_pending(&mut app_state) {
//                                 Ok(_) => { log::trace!("Wayland events dispatched successfully."); }
//                                 Err(e) => {
//                                     log::error!("Error in dispatch_pending: {}", e);
//                                     // Handle Wayland dispatch errors similarly to before
//                                     match e {
//                                         wayland_client::DispatchError::Backend(WaylandError::Io(io_err)) => {
//                                             if io_err.kind() == std::io::ErrorKind::Interrupted {
//                                                 log::warn!("Wayland dispatch_pending interrupted (IO), continuing.");
//                                             } else {
//                                                 log::error!("Wayland dispatch_pending IO error: {}, exiting.", io_err);
//                                                 app_state.running = false;
//                                             }
//                                         }
//                                         wayland_client::DispatchError::Backend(WaylandError::Protocol(protocol_err)) => {
//                                             log::error!("Wayland dispatch_pending protocol error: {}, exiting.", protocol_err);
//                                             app_state.running = false;
//                                         }
//                                         _ => {
//                                             log::error!("Unhandled Wayland dispatch_pending error: {}, exiting.", e);
//                                             app_state.running = false;
//                                         }
//                                     }
//                                 }
//                             }
//                         }
//                         Err(WaylandError::Io(io_err)) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
//                             log::trace!("Wayland read would block, no new events this cycle (after poll).");
//                         }
//                         Err(WaylandError::Io(io_err)) => {
//                             log::error!("Error reading from Wayland connection (after poll): {}", io_err);
//                             app_state.running = false;
//                         }
//                         Err(e) => {
//                             log::error!("Error reading from Wayland connection (non-IO, after poll): {}", e);
//                             app_state.running = false;
//                         }
//                     }
//                 } else {
//                     log::warn!("Failed to prepare_read on Wayland connection after poll.");
//                 }
//             }
//             if (fds[WAYLAND_FD_IDX].revents & libc::POLLERR) != 0 || (fds[WAYLAND_FD_IDX].revents & libc::POLLHUP) != 0 {
//                  log::error!("Wayland FD error/hangup (POLLERR/POLLHUP). Exiting.");
//                  app_state.running = false;
//             }
//
//
//             // Check libinput FD
//             if let Some(libinput_idx) = libinput_fd_idx_opt {
//                 if app_state.running && (fds[libinput_idx].revents & libc::POLLIN) != 0 {
//                     log::trace!("Libinput FD readable (POLLIN)");
//                     if let Some(ref mut context) = app_state.input_context {
//                         if context.dispatch().is_err() {
//                             log::error!("Libinput dispatch error in event processing loop");
//                         }
//                         while let Some(libinput_event) = context.next() {
//                             if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = libinput_event {
//                                 let key_code = key_event.key();
//                                 let key_state = key_event.key_state();
//                                 log::trace!("Key event: code {}, state {:?}", key_code, key_state);
//
//                                 let pressed = key_state == KeyState::Pressed;
//                                 if let Some(current_state) = app_state.key_states.get_mut(&key_code) {
//                                     if *current_state != pressed {
//                                         *current_state = pressed;
//                                         needs_redraw = true;
//                                         log::info!("Configured key {} state changed: {}", key_code, pressed);
//                                     }
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//
//         if !app_state.running {
//             break;
//         }
//
//         if needs_redraw {
//             if app_state.surface.is_some() && app_state.compositor.is_some() && app_state.shm.is_some() {
//                  app_state.draw(&qh);
//             } else {
//                 log::warn!("Skipping draw due to uninitialized Wayland components.");
//             }
//         }
//
//         match conn.flush() {
//             Ok(_) => { log::trace!("Wayland connection flushed successfully."); }
//             Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
//                 log::warn!("Wayland flush would block. Messages might be delayed.");
//             }
//             Err(e) => {
//                 log::error!("Failed to flush Wayland connection: {}", e);
//                 app_state.running = false;
//             }
//         }
//
//         // The main loop using libc::poll doesn't require explicit re-registration of FDs in this manner.
//         // The fds array is re-used in each call to libc::poll.
//     }
//     log::info!("Exiting application loop.");
// }


// Modified main function (this one should be the active one)
fn main() {
    let cli = Cli::parse();

    // Initialize logger early for any messages during config loading or --check
    // but allow --check to proceed even if full env_logger setup is complex.
    // For --check, simple prints might be enough, but logs can be helpful.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .err()
        .map(|e| eprintln!("Failed to initialize logger: {}. Continuing without detailed logging for --check.", e));


    let config_path = &cli.config;
    let config_content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read configuration file '{}': {}", config_path, e);
            process::exit(1);
        }
    };

    let mut app_config: AppConfig = match toml::from_str(&config_content) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to parse TOML configuration from '{}': {}", config_path, e);
            process::exit(1);
        }
    };

    // Process raw_keycode to populate keycode (common for both --check and normal run)
    let mut keycode_resolution_errors = Vec::new();
    for key_conf in app_config.key.iter_mut() {
        if key_conf.width <= 0.0 {
            keycode_resolution_errors.push(format!("Key '{}' has invalid width: {}", key_conf.name, key_conf.width));
        }
        if key_conf.height <= 0.0 {
             keycode_resolution_errors.push(format!("Key '{}' has invalid height: {}", key_conf.name, key_conf.height));
        }

        let resolved_code = match key_conf.raw_keycode.as_ref() {
            Some(SerdeValue::String(s)) => keycodes::get_keycode_from_string(s),
            Some(SerdeValue::U8(i)) => Ok(*i as u32),
            Some(SerdeValue::U16(i)) => Ok(*i as u32),
            Some(SerdeValue::U32(i)) => Ok(*i),
            Some(SerdeValue::U64(i)) => if *i <= u32::MAX as u64 { Ok(*i as u32) } else { Err(format!("Integer keycode {} for key '{}' is too large for u32.", i, key_conf.name)) },
            Some(SerdeValue::I8(i)) => if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) },
            Some(SerdeValue::I16(i)) => if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) },
            Some(SerdeValue::I32(i)) => if *i >= 0 { Ok(*i as u32) } else { Err(format!("Negative keycode {} for key '{}' is invalid.", i, key_conf.name)) },
            Some(SerdeValue::I64(i)) => if *i >= 0 && *i <= u32::MAX as i64 { Ok(*i as u32) } else { Err(format!("Integer keycode {} for key '{}' is out of valid u32 range.", i, key_conf.name)) },
            None => keycodes::get_keycode_from_string(&key_conf.name),
            Some(other_type) => Err(format!("Invalid type for keycode field for key '{}': expected string or integer, got {:?}", key_conf.name, other_type)),
        };

        match resolved_code {
            Ok(code) => key_conf.keycode = code,
            Err(e) => {
                let error_msg = if key_conf.raw_keycode.is_none() {
                    format!(
                        "Error processing key '{}': Could not resolve default keycode from name ('{}'). Please specify a 'keycode' field. Details: {}",
                        key_conf.name, key_conf.name, e
                    )
                } else {
                    format!("Error processing keycode for key '{}': {}", key_conf.name, e)
                };
                keycode_resolution_errors.push(error_msg);
            }
        }
    }

    if !keycode_resolution_errors.is_empty() {
        eprintln!("Errors found during keycode resolution:");
        for err in keycode_resolution_errors {
            eprintln!("- {}", err);
        }
        process::exit(1);
    }
    // End of common config processing part needed for --check too


    if cli.check {
        println!("Performing configuration check for '{}'...", config_path);

        // Validate configuration (overlapping keys, duplicate keycodes, etc.)
        if let Err(e) = validate_config(&app_config) {
            eprintln!("Configuration validation failed: {}", e);
            process::exit(1);
        } else {
            println!("Basic validation (overlaps, duplicates, positive dimensions) passed.");
        }

        // Load font for text metrics
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
        println!("{:<20} | {:<25} | {:<10} | {:<10} | {:<15} | {:<10}",
                 "Label (Name)", "Bounding Box (L,T,W,H)", "Keycode", "FontSz(pts)", "Final Text", "Truncated");
        println!("{:-<20}-+-{:-<25}-+-{:-<10}-+-{:-<10}-+-{:-<15}-+-{:-<10}", "", "", "", "", "", "");

        for key_config in &app_config.key {
            let bbox_str = format!("{:.1},{:.1}, {:.1}x{:.1}",
                                   key_config.left, key_config.top,
                                   key_config.width, key_config.height);

            let initial_font_size = key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED);

            match simulate_text_layout(key_config, &ft_face) {
                Ok(text_check_result) => {
                    println!("{:<20} | {:<25} | {:<10} | {:<10.1} | {:<15} | {:<10}",
                             key_config.name,
                             bbox_str,
                             key_config.keycode,
                             text_check_result.final_font_size_pts, // Show the font size that was actually used for metrics
                             format!("'{}'",text_check_result.final_text),
                             text_check_result.truncated_chars
                        );
                }
                Err(e) => {
                     println!("{:<20} | {:<25} | {:<10} | {:<10.1} | Error simulating text: {} ",
                             key_config.name,
                             bbox_str,
                             key_config.keycode,
                             initial_font_size,
                             e
                        );
                }
            }
        }

        println!("\nConfiguration check finished.");
        process::exit(0);
    }

    // Proceed with normal application startup if --check is not present
    // (Logger was already initialized)
    log::info!("Starting Wayland application with config '{}'...", config_path);
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

    log::trace!("Dispatching initial events to bind globals...");
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during initial roundtrip for global binding.");
        process::exit(1);
    }

    if app_state.compositor.is_none() || app_state.shm.is_none() || app_state.xdg_wm_base.is_none() {
        log::error!("Failed to bind essential Wayland globals (wl_compositor, wl_shm, xdg_wm_base).");
        eprintln!("Could not bind essential Wayland globals. This usually means the Wayland compositor is missing support or encountered an issue.");
        process::exit(1);
    }
    log::info!("Essential Wayland globals bound.");

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
            // Do not exit, allow OSD to run visually even without input.
        }
    }

    let surface = app_state.compositor.as_ref().unwrap().create_surface(&qh, ());
    app_state.surface = Some(surface.clone());
    let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("Wayland Keyboard OSD".to_string());
    surface.commit(); // Commit to make the surface known to the compositor

    // Dispatch events once to process initial configure and draw the window.
    log::info!("Initial surface commit done. Dispatching events to catch initial XDG configure...");
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during roundtrip after surface commit (waiting for initial configure).");
        // Depending on the error, might want to exit or handle differently
    }
    // An explicit draw call here might be needed if the first configure doesn't trigger it,
    // or if we want to show something before the first configure.
    // However, the xdg_surface configure event should trigger the first draw.
    log::info!("Initial roundtrip complete. Wayland window should be configured. Waiting for events...");


    use input::event::Event as LibinputEvent;
    use input::event::keyboard::{KeyboardEvent, KeyState, KeyboardEventTrait};
    use wayland_client::backend::WaylandError;
    use std::os::unix::io::{AsRawFd as _, RawFd};
    use std::io;

    log::info!("Entering main event loop.");

    let wayland_raw_fd: RawFd = match conn.prepare_read() {
        Some(guard) => guard.connection_fd().as_raw_fd(),
        None => {
            log::error!("Wayland connection is dead before starting event loop.");
            process::exit(1);
        }
    };

    let mut fds: Vec<libc::pollfd> = Vec::new();
    fds.push(libc::pollfd { fd: wayland_raw_fd, events: libc::POLLIN, revents: 0 });
    const WAYLAND_FD_IDX: usize = 0;

    let mut libinput_fd_idx_opt: Option<usize> = None;
    if let Some(ref context) = app_state.input_context {
        let libinput_raw_fd: RawFd = context.as_raw_fd();
        fds.push(libc::pollfd { fd: libinput_raw_fd, events: libc::POLLIN, revents: 0 });
        libinput_fd_idx_opt = Some(fds.len() - 1);
        log_if_input_device_access_denied(&app_state.config, context);
    } else {
        log::warn!("No libinput context available. Key press/release events will not be monitored.");
    }


    let poll_timeout_ms = 100; // Timeout for poll in milliseconds

    while app_state.running {
        for item in fds.iter_mut() { item.revents = 0; }
        let mut needs_redraw = false;

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, poll_timeout_ms) };

        if ret < 0 {
            let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if errno == libc::EINTR { continue; }
            log::error!("libc::poll error: {}", io::Error::last_os_error());
            app_state.running = false; break;
        } else if ret == 0 {
            // Timeout
        } else {
            // Wayland events
            if (fds[WAYLAND_FD_IDX].revents & libc::POLLIN) != 0 {
                if let Some(guard) = conn.prepare_read() {
                    match guard.read() { // Corrected from read_events() to read()
                        Ok(bytes_read) => { // read() returns number of bytes or an error
                            log::trace!("Successfully read {} bytes from Wayland socket", bytes_read);
                             match event_queue.dispatch_pending(&mut app_state) {
                                Ok(_) => {}
                                Err(e) => { log::error!("Error dispatching Wayland events: {}", e); app_state.running = false; }
                            }
                        }
                        Err(WaylandError::Io(io_err)) if io_err.kind() == std::io::ErrorKind::WouldBlock => { /* No new events */ }
                        Err(e) => { log::error!("Error reading from Wayland connection: {}", e); app_state.running = false; }
                    }
                } else {
                     log::error!("Wayland connection read guard acquisition failed after poll.");
                     app_state.running = false;
                }
            }
            if (fds[WAYLAND_FD_IDX].revents & (libc::POLLERR | libc::POLLHUP)) != 0 {
                 log::error!("Wayland FD error/hangup (POLLERR/POLLHUP). Exiting.");
                 app_state.running = false;
            }

            // Libinput events
            if let Some(libinput_idx) = libinput_fd_idx_opt {
                if app_state.running && (fds[libinput_idx].revents & libc::POLLIN) != 0 {
                    if let Some(ref mut context) = app_state.input_context {
                        if context.dispatch().is_err() { log::error!("Libinput dispatch error"); }
                        while let Some(event) = context.next() {
                            if let LibinputEvent::Keyboard(KeyboardEvent::Key(key_event)) = event {
                                let key_code = key_event.key();
                                let pressed = key_event.key_state() == KeyState::Pressed;
                                if let Some(current_state) = app_state.key_states.get_mut(&key_code) {
                                    if *current_state != pressed {
                                        *current_state = pressed;
                                        needs_redraw = true;
                                        log::debug!("Key {} state changed to: {}", key_code, pressed);
                                    }
                                }
                            }
                        }
                    }
                }
                 if app_state.running && (fds[libinput_idx].revents & (libc::POLLERR | libc::POLLHUP)) != 0 {
                    log::error!("Libinput FD error/hangup (POLLERR/POLLHUP). Input monitoring might stop.");
                    // Optionally remove libinput_fd from poll list or handle context recreation
                    if let Some(ref mut context) = app_state.input_context {
                        // Attempt to dispatch any remaining events.
                        let _ = context.dispatch();
                    }
                    app_state.input_context = None; // Stop trying to use it.
                    if fds.len() > libinput_idx { // Ensure index is valid before removing
                        fds.remove(libinput_idx);
                    }
                    libinput_fd_idx_opt = None; // Clear the index.
                    log::warn!("Libinput context removed due to FD error. Key press/release events will no longer be monitored.");

                }
            }
        }

        if !app_state.running { break; }

        if needs_redraw {
            if app_state.surface.is_some() { app_state.draw(&qh); }
        }

        match conn.flush() {
            Ok(_) => {}
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => { /* Fine */ }
            Err(e) => { log::error!("Failed to flush Wayland connection: {}", e); app_state.running = false; }
        }
    }
    log::info!("Exiting application loop.");
}

// Helper to log if input devices are inaccessible after attempting to assign seat
// This is a common issue and providing a hint can be useful.
fn log_if_input_device_access_denied(app_config: &AppConfig, libinput_context: &input::Libinput) {
    // This is a heuristic. If we have keys configured but libinput doesn't report any devices,
    // it's a strong indicator of a permissions problem.
    // A more direct check isn't easily available through libinput's public API after context creation.
    // We can try to iterate devices, but libinput might not even list them if it can't open them.
    // The errors during open_restricted are logged by MyLibinputInterface.
    // This function is more about a summary warning if things look suspicious.

    // let mut device_count = 0; // Unused
    let mut temp_context = libinput_context.clone(); // Clone to iterate without affecting the main one
    while let Some(_event) = temp_context.next() {
        // We are just trying to see if there are any devices.
        // A DeviceAdded event would be ideal, but iterating and checking type is also an option.
        // However, libinput.next() consumes events. A non-event way to count devices is needed.
        // Libinput's internal device list isn't directly exposed.
        // For now, we rely on the errors logged by `open_restricted`.
        // This function could be enhanced if a better check is found.
    }
    // A simpler check: if `udev_assign_seat` succeeded but `open_restricted` failed for all relevant devices,
    // that's a problem. MyLibinputInterface logs those failures.
    // This function's main purpose now is to remind the user if config expects input but context might be "empty".
    if !app_config.key.is_empty() {
        // A definitive way to check if devices are usable is hard without iterating them.
        // Relying on `MyLibinputInterface` logs for specific device open errors.
        // This is a general reminder if input is expected.
        log::debug!("Input context initialized. Check previous logs for any 'Failed to open path' errors from libinput if keys don't respond.");
    }
}
