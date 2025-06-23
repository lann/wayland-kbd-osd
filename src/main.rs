use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::protocol::{wl_compositor, wl_shm, wl_shm_pool, wl_surface, wl_buffer, wl_registry, wl_output};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use input; // Added for libinput

use std::collections::HashMap; // For storing key states and configs by keycode
// Serde for TOML deserialization
use serde::Deserialize;
use serde_value::Value as SerdeValue; // For flexible keycode parsing
use std::fs; // For file reading
use std::process; // For exiting gracefully on config error
use clap::Parser; // For command-line argument parsing
use wayland_client::backend::WaylandError; // Added for handle_wayland_events
use input::event::Event as LibinputEvent; // Added for handle_libinput_events
use input::event::keyboard::{KeyboardEvent, KeyState, KeyboardEventTrait}; // Added for handle_libinput_events

mod keycodes; // Our new module

// Graphics and Font rendering
use cairo::{Context, ImageSurface, Format, FontFace as CairoFontFace};
use freetype::{Library as FreeTypeLibrary};

// Configuration Structs

// Represents a size that can be absolute (pixels) or relative (ratio of screen)
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(untagged)] // Allows parsing "100" as pixels(100) or "0.5" as ratio(0.5)
enum SizeDimension {
    Pixels(u32),
    Ratio(f32),
}

// Enum for specifying overlay position
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum OverlayPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    CenterLeft,
    CenterRight,
}

#[derive(Deserialize, Debug, Clone)]
struct OverlayConfig {
    #[serde(default)]
    screen: Option<String>, // Monitor name or index (string for now, parse later)
    #[serde(default = "default_overlay_position")]
    position: OverlayPosition,
    size_width: Option<SizeDimension>,
    size_height: Option<SizeDimension>,
    #[serde(default = "default_overlay_margin")]
    margin_top: i32,
    #[serde(default = "default_overlay_margin")]
    margin_right: i32,
    #[serde(default = "default_overlay_margin")]
    margin_bottom: i32,
    #[serde(default = "default_overlay_margin")]
    margin_left: i32,
    #[serde(default = "default_background_color_inactive")]
    background_color_inactive: String,
    #[serde(default = "default_background_color_active")]
    background_color_active: String, // Note: "active" here refers to the window, not key press
                                     // For now, only inactive is used for the general overlay background.
                                     // "active" could be used if the overlay itself had focus states.
}

fn default_overlay_position() -> OverlayPosition {
    OverlayPosition::BottomCenter
}

fn default_overlay_margin() -> i32 {
    0
}

fn default_background_color_inactive() -> String {
    "#00000080".to_string() // Translucent black
}

fn default_background_color_active() -> String {
    "#A0A0A0D0".to_string() // Slightly more opaque grey (currently unused for global background)
}

impl Default for OverlayConfig {
    fn default() -> Self {
        OverlayConfig {
            screen: None,
            position: default_overlay_position(),
            size_width: None, // No default width, derive from height or layout
            size_height: Some(SizeDimension::Ratio(0.3)), // Default to 30% screen height
            margin_top: default_overlay_margin(),
            margin_right: default_overlay_margin(),
            margin_bottom: default_overlay_margin(),
            margin_left: default_overlay_margin(),
            background_color_inactive: default_background_color_inactive(),
            background_color_active: default_background_color_active(),
        }
    }
}

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
    #[serde(default)]
    key: Vec<KeyConfig>,
    #[serde(default)]
    overlay: OverlayConfig, // Add the new overlay configuration
}

// Helper function to parse color string like "#RRGGBBAA" or "#RGB"
// Returns (r, g, b, a) tuple with values from 0.0 to 1.0
fn parse_color_string(color_str: &str) -> Result<(f64, f64, f64, f64), String> {
    let s = color_str.trim_start_matches('#');
    match s.len() {
        6 => { // RRGGBB
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| format!("Invalid hex for B: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0)) // Default alpha to 1.0
        }
        8 => { // RRGGBBAA
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| format!("Invalid hex for B: {}", e))?;
            let a = u8::from_str_radix(&s[6..8], 16).map_err(|e| format!("Invalid hex for A: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, a as f64 / 255.0))
        }
        3 => { // RGB
            let r_char = s.chars().nth(0).unwrap();
            let g_char = s.chars().nth(1).unwrap();
            let b_char = s.chars().nth(2).unwrap();
            let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16).map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16).map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16).map_err(|e| format!("Invalid hex for B: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0))
        }
        4 => { // RGBA
            let r_char = s.chars().nth(0).unwrap();
            let g_char = s.chars().nth(1).unwrap();
            let b_char = s.chars().nth(2).unwrap();
            let a_char = s.chars().nth(3).unwrap();
            let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16).map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16).map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16).map_err(|e| format!("Invalid hex for B: {}", e))?;
            let a = u8::from_str_radix(&format!("{}{}", a_char, a_char), 16).map_err(|e| format!("Invalid hex for A: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, a as f64 / 255.0))
        }
        _ => Err(format!("Invalid color string length for '{}'. Expected #RRGGBB, #RRGGBBAA, #RGB, or #RGBA", color_str)),
    }
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
    // This section attempts to fit the key's text within its bounds.
    // It involves two main strategies:
    // 1. Font Size Scaling: Reduce font size iteratively until text fits or min font size is reached.
    // 2. Text Truncation: If scaling isn't enough, remove characters from the end and append "..."

    let mut current_text = key.text.clone();
    let mut current_font_size = key.text_size as f64;
    ctx.set_font_size(current_font_size); // Set initial font size

    // Define text area constraints based on key dimensions and padding.
    // Padding is a small percentage of the key's width/height, with a minimum pixel value.
    let text_padding = (key.width * 0.1).min(key.height * 0.1).max(2.0) as f64;
    let max_text_width = width - 2.0 * text_padding; // Available width for text inside padding
    // let max_text_height = height - 2.0 * text_padding; // Max height could also be a constraint if needed

    let original_font_size = key.text_size as f64;
    // Define a minimum sensible font size: 50% of original, but not less than 6.0 points.
    let min_font_size = (original_font_size * 0.5).max(6.0);

    let mut text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (initial)");

    // --- Stage 1: Font Size Scaling ---
    // Iteratively reduce font size if the current text width exceeds the maximum allowed width.
    // Stop if text fits, or if the font size reaches the defined minimum.
    while text_extents.width() > max_text_width && current_font_size > min_font_size {
        current_font_size *= 0.9; // Reduce font size by 10%
        if current_font_size < min_font_size {
            current_font_size = min_font_size; // Clamp to minimum font size
        }
        ctx.set_font_size(current_font_size); // Apply new font size to context
        text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (scaling)");

        // If even at minimum font size the text is too wide, break and proceed to truncation.
        if current_font_size == min_font_size && text_extents.width() > max_text_width {
            break;
        }
    }

    // --- Stage 2: Text Truncation ---
    // If, after font scaling, the text is still too wide, truncate it.
    if text_extents.width() > max_text_width {
        let ellipsis = "...";
        let ellipsis_extents = ctx.text_extents(ellipsis).expect("Failed to get ellipsis extents");
        // Calculate the maximum width available for the text part when an ellipsis is present.
        let max_width_for_text_with_ellipsis = max_text_width - ellipsis_extents.width();

        // Iteratively remove characters from the end of `current_text`.
        while text_extents.width() > max_text_width && !current_text.is_empty() {
            if current_text.pop().is_none() { // Remove the last character
                break; // Should not happen due to !current_text.is_empty() check, but good for safety.
            }

            // Form a temporary string with the current (shortened) text plus ellipsis.
            let temp_text_with_ellipsis = if current_text.is_empty() {
                // If all original text is removed, show ellipsis if it fits, otherwise empty string.
                if ellipsis_extents.width() <= max_text_width { ellipsis.to_string() } else { "".to_string() }
            } else {
                format!("{}{}", current_text, ellipsis)
            };

            // Measure the new temporary string.
            text_extents = ctx.text_extents(&temp_text_with_ellipsis).expect("Failed to get text extents (truncating)");

            // Check if the actual text part (without ellipsis) now fits within its allocated space.
            let current_text_only_extents = ctx.text_extents(&current_text).expect("Failed to get current_text extents");
            if current_text_only_extents.width() <= max_width_for_text_with_ellipsis || current_text.is_empty() {
                 current_text = temp_text_with_ellipsis; // Adopt the text with ellipsis
                 text_extents = ctx.text_extents(&current_text).expect("Failed to get final truncated text extents");
                 break; // Text with ellipsis fits, or text is empty.
            }
        }

        // --- Stage 2b: Final Ellipsis Fit Check ---
        // After truncation loop, if `current_text` (which might include "...") is still too wide,
        // it means even the shortest version of `text...` didn't fit.
        // Try to fit just the ellipsis, then shorter versions ("..", "."), then empty string.
        if text_extents.width() > max_text_width {
            if ellipsis_extents.width() <= max_text_width { // Can full "..." fit?
                current_text = ellipsis.to_string();
            } else if ctx.text_extents("..").unwrap().width() <= max_text_width { // Can ".." fit?
                current_text = "..".to_string();
            } else if ctx.text_extents(".").unwrap().width() <= max_text_width { // Can "." fit?
                current_text = ".".to_string();
            } else { // Nothing fits.
                current_text = "".to_string();
            }
            // text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (final truncation check)"); // Recalculate if needed, but current_text is final.
        }
    }
    // --- End of Text Scaling and Truncation ---

    // Recalculate text_extents with the final potentially scaled/truncated text and font size.
    // ctx.set_font_size(current_font_size); // Font size is already set correctly from scaling/truncation phase.
    let text_extents = ctx.text_extents(&current_text).expect("Failed to get text extents (final)");


    // Calculate text position to center it
    let text_x = (width - text_extents.width()) / 2.0 - text_extents.x_bearing();
    let text_y = (height - text_extents.height()) / 2.0 - text_extents.y_bearing();

    ctx.move_to(text_x, text_y);
    ctx.show_text(&current_text).expect("Cairo show_text failed");

    ctx.restore().expect("Failed to restore cairo context state");
}

use std::os::unix::io::{AsRawFd, OwnedFd};
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

// Struct to hold output information
#[derive(Debug, Clone, Default)]
struct OutputInfo {
    name: Option<String>,
    description: Option<String>,
    logical_width: i32,
    logical_height: i32,
    // Add other fields like scale factor if needed later
}


// Dispatcher for zwlr_layer_surface_v1
impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut AppState,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                log::info!(
                    "LayerSurface Configure: serial: {}, width: {}, height: {}",
                    serial, width, height
                );
                // Update size if compositor suggests a new one and it's not zero
                if width > 0 { state.configured_width = width as i32; }
                if height > 0 { state.configured_height = height as i32; }

                // Client must ack the configure
                layer_surface.ack_configure(serial);

                // Mark that a redraw is needed due to potential size change
                state.needs_redraw = true;
                // Explicitly draw if the surface is configured and ready
                if state.surface.is_some() {
                     log::debug!("LayerSurface Configure event: triggering draw and setting needs_redraw to false.");
                     state.draw(qh); // Draw immediately
                     state.needs_redraw = false; // Reset as draw was just called
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                log::info!("LayerSurface Closed event received. Application will exit.");
                state.running = false; // Signal the main loop to stop
            }
            _ => {
                log::trace!("Unhandled zwlr_layer_surface_v1 event: {:?}", event);
            }
        }
    }
}

struct AppState {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    xdg_output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    outputs: Vec<(u32, wl_output::WlOutput, Option<zxdg_output_v1::ZxdgOutputV1>, OutputInfo)>, // Store (name, output_obj, xdg_output_obj, info)
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    buffer: Option<wl_buffer::WlBuffer>,
    mmap: Option<MmapMut>,
    temp_file: Option<std::fs::File>, // Added for persistent temp file
    configured_width: i32,
    configured_height: i32,
    running: bool, // Added to control the main loop
    input_context: Option<input::Libinput>,
    config: AppConfig,               // Store loaded configuration
    key_states: HashMap<u32, bool>,  // Stores pressed state for each configured keycode
    needs_redraw: bool, // Added to manage redraw logic globally

    // Cache for layout calculations
    last_draw_width: i32,
    last_draw_height: i32,
    cached_scale: f32,
    cached_offset_x: f32,
    cached_offset_y: f32,
    layout_cache_valid: bool,
}

impl AppState {
    fn new(app_config: AppConfig) -> Self {
        let mut key_states_map = HashMap::new();
        for key_conf in app_config.key.iter() {
            key_states_map.insert(key_conf.keycode, false);
        }

        AppState {
            compositor: None,
            shm: None,
            xdg_wm_base: None,
            layer_shell: None,
            xdg_output_manager: None,
            outputs: Vec::new(),
            surface: None,
            layer_surface: None,
            buffer: None,
            mmap: None,
            temp_file: None,
            configured_width: WINDOW_WIDTH,
            configured_height: WINDOW_HEIGHT,
            running: true,
            input_context: None,
            config: app_config,
            key_states: key_states_map,
            needs_redraw: true, // Start with true to ensure initial draw

            last_draw_width: 0,
            last_draw_height: 0,
            cached_scale: 1.0,
            cached_offset_x: 0.0,
            cached_offset_y: 0.0,
            layout_cache_valid: false,
        }
    }

    // Calculates the bounding box of the raw key layout from the config.
    // Returns (width, height) of the layout in abstract units.
    // Returns (0.0, 0.0) if no keys are configured.
    fn get_key_layout_bounds(&self) -> (f32, f32) {
        if self.config.key.is_empty() {
            return (0.0, 0.0);
        }

        let mut min_coord_x = f32::MAX;
        let mut max_coord_x = f32::MIN;
        let mut min_coord_y = f32::MAX;
        let mut max_coord_y = f32::MIN;

        for key_config in &self.config.key {
            min_coord_x = min_coord_x.min(key_config.left);
            max_coord_x = max_coord_x.max(key_config.left + key_config.width);
            min_coord_y = min_coord_y.min(key_config.top);
            max_coord_y = max_coord_y.max(key_config.top + key_config.height);
        }

        let layout_width = max_coord_x - min_coord_x;
        let layout_height = max_coord_y - min_coord_y;
        (layout_width.max(0.0), layout_height.max(0.0))
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

            let shm_temp_file = tempfile::tempfile().expect("Failed to create temp file for SHM");
            shm_temp_file.set_len(size as u64).expect("Failed to set SHM temp file length");
            self.mmap = Some(unsafe { MmapMut::map_mut(&shm_temp_file).expect("Failed to map SHM temp file") });
            self.temp_file = Some(shm_temp_file);
        }

        // self.mmap is guaranteed to be Some by the logic above.
        // let mmap = self.mmap.as_mut().unwrap(); // This variable was unused, mmap_data below is used.
        let shm_temp_file_fd = self.temp_file.as_ref().unwrap().as_raw_fd();

        // Create a new pool and buffer for each draw. This is typical.
        let pool = shm.create_pool(shm_temp_file_fd, size as i32, qh, ());
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

        // Clear the surface with configured background color
        let bg_color_str = &self.config.overlay.background_color_inactive;
        match parse_color_string(bg_color_str) {
            Ok((r, g, b, a)) => {
                ctx.save().unwrap();
                ctx.set_source_rgba(r, g, b, a);
                ctx.set_operator(cairo::Operator::Source); // Replace content
                ctx.paint().expect("Cairo paint (clear) failed");
                ctx.restore().unwrap();
            }
            Err(e) => {
                log::error!("Failed to parse background_color_inactive '{}': {}. Using default transparent.", bg_color_str, e);
                // Fallback to default transparent clear if parsing fails
                ctx.save().unwrap();
                ctx.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                ctx.set_operator(cairo::Operator::Source);
                ctx.paint().expect("Cairo paint (clear fallback) failed");
                ctx.restore().unwrap();
            }
        }

        let scale: f32;
        let offset_x: f32;
        let offset_y: f32;

        if self.config.key.is_empty() {
            log::warn!("No keys configured. Nothing to draw.");
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, width, height);
            surface.commit();
            self.buffer = Some(buffer);
            return;
        }

        // Check if layout parameters can be reused from cache
        if self.layout_cache_valid && self.configured_width == self.last_draw_width && self.configured_height == self.last_draw_height {
            scale = self.cached_scale;
            offset_x = self.cached_offset_x;
            offset_y = self.cached_offset_y;
            log::trace!("Using cached layout parameters: scale={}, offset_x={}, offset_y={}", scale, offset_x, offset_y);
        } else {
            log::trace!("Recalculating layout parameters. Cache invalid or dimensions changed.");
            let mut min_coord_x = f32::MAX;
            let mut max_coord_x = f32::MIN;
            let mut min_coord_y = f32::MAX;
            let mut max_coord_y = f32::MIN;

            for key_config in &self.config.key {
                min_coord_x = min_coord_x.min(key_config.left);
                max_coord_x = max_coord_x.max(key_config.left + key_config.width);
                min_coord_y = min_coord_y.min(key_config.top);
                max_coord_y = max_coord_y.max(key_config.top + key_config.height);
            }

            let layout_width = max_coord_x - min_coord_x;
            let layout_height = max_coord_y - min_coord_y;

            // Adjust padding: minimal if overlay size is configured, else dynamic.
            let padding = if self.config.overlay.size_width.is_some() || self.config.overlay.size_height.is_some() {
                2.0 // Minimal padding when size is explicitly configured
            } else {
                (width.min(height) as f32 * 0.05).max(5.0) // Original dynamic padding
            };
            log::trace!("Using padding: {}", padding);

            let drawable_width = (width as f32 - 2.0 * padding).max(0.0); // Ensure non-negative
            let drawable_height = (height as f32 - 2.0 * padding).max(0.0); // Ensure non-negative

            let current_scale_x = if layout_width > 0.0 { drawable_width / layout_width } else { 1.0 };
            let current_scale_y = if layout_height > 0.0 { drawable_height / layout_height } else { 1.0 };
            scale = current_scale_x.min(current_scale_y).max(0.01);

            let scaled_layout_width = layout_width * scale;
            let scaled_layout_height = layout_height * scale;
            offset_x = padding + (drawable_width - scaled_layout_width) / 2.0 - (min_coord_x * scale);
            offset_y = padding + (drawable_height - scaled_layout_height) / 2.0 - (min_coord_y * scale);

            // Update cache
            self.cached_scale = scale;
            self.cached_offset_x = offset_x;
            self.cached_offset_y = offset_y;
            self.last_draw_width = self.configured_width;
            self.last_draw_height = self.configured_height;
            self.layout_cache_valid = true;
            log::trace!("Updated layout cache: scale={}, offset_x={}, offset_y={}", scale, offset_x, offset_y);
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

        // Default appearance values are now global consts.
        // DEFAULT_CORNER_RADIUS_UNSCALED
        // DEFAULT_BORDER_THICKNESS_UNSCALED
        // DEFAULT_ROTATION_DEGREES
        // DEFAULT_TEXT_SIZE_UNSCALED

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

// Default appearance values (unscaled) - also used in check_config and AppState::draw
const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;
const DEFAULT_ROTATION_DEGREES: f32 = 0.0;
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
    fn event( _state: &mut AppState, proxy: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _data: &(), _conn: &Connection, _qh: &QueueHandle<AppState>) {
        if let xdg_wm_base::Event::Ping { serial } = event { proxy.pong(serial); }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
    fn event( state: &mut AppState, surface_proxy: &xdg_surface::XdgSurface, event: xdg_surface::Event, _data: &(), _conn: &Connection, qh: &QueueHandle<AppState>) {
        if let xdg_surface::Event::Configure { serial, .. } = event {
            surface_proxy.ack_configure(serial);
            if state.surface.is_some() {
                log::debug!("XDG Surface Configure event: triggering draw and setting needs_redraw to false.");
                state.draw(qh);
                state.needs_redraw = false; // The configure event's draw handles the current need.
            }
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
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, (), AppState>(name, std::cmp::min(version,4), qh, ()));
                    log::info!("Bound zwlr_layer_shell_v1 version {}", std::cmp::min(version,4));
                }
                "zxdg_output_manager_v1" => {
                    // xdg-output-manager is version 3 in wayland-protocols 0.30
                    state.xdg_output_manager = Some(registry.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, (), AppState>(name, std::cmp::min(version,3), qh, ()));
                    log::info!("Bound zxdg_output_manager_v1 version {}", std::cmp::min(version,3));
                }
                "wl_output" => {
                    let output_obj = registry.bind::<wl_output::WlOutput, _, _>(name, std::cmp::min(version, 4), qh, ()); // Version 4 is current max for wl_output
                    log::info!("Bound wl_output (id: {}) version {}", output_obj.id(), std::cmp::min(version, 4));

                    let xdg_output_obj_opt = if let Some(manager) = state.xdg_output_manager.as_ref() {
                        // xdg-output is version 3
                        let xdg_output = manager.get_xdg_output(&output_obj, qh, ());
                        log::info!("Created zxdg_output_v1 (id: {}) for wl_output (id: {})", xdg_output.id(), output_obj.id());
                        Some(xdg_output)
                    } else {
                        log::warn!("zxdg_output_manager_v1 not available when wl_output was bound. Cannot get zxdg_output_v1.");
                        None
                    };
                    state.outputs.push((name, output_obj, xdg_output_obj_opt, OutputInfo::default()));
                }
                _ => {
                    log::trace!("Ignoring unknown global: {} version {}", interface, version);
                }
            }
        } else if let wl_registry::Event::GlobalRemove { name } = event {
            log::info!("Global removed: ID {}", name);
            state.outputs.retain(|(output_name, _, _, _)| *output_name != name);
            // TODO: Consider if we need to None-out other globals if they are removed, e.g. layer_shell
        }
    }
}

// Dispatcher for wl_output
impl Dispatch<wl_output::WlOutput, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        match event {
            wl_output::Event::Geometry { x, y, physical_width, physical_height, subpixel, make, model, transform } => {
                log::debug!("wl_output Geometry: x={}, y={}, physical_width={}, physical_height={}, subpixel={:?}, make={}, model={}, transform={:?}",
                    x, y, physical_width, physical_height, subpixel, make, model, transform);
            }
            wl_output::Event::Mode { flags, width, height, refresh } => {
                log::debug!("wl_output Mode: flags={:?}, width={}, height={}, refresh={}", flags, width, height, refresh);
            }
            wl_output::Event::Done => {
                log::debug!("wl_output Done");
            }
            wl_output::Event::Scale { factor } => {
                log::debug!("wl_output Scale: factor={}", factor);
            }
            wl_output::Event::Name { name } => {
                // This is for wl_output name, which is often not very descriptive (e.g., "wayland-0")
                 log::debug!("wl_output Name: {}", name);
            }
            wl_output::Event::Description { description } => {
                // This is for wl_output description, usually more descriptive
                 log::debug!("wl_output Description: {}", description);
            }
            _ => {
                log::trace!("Unhandled wl_output event: {:?}", event);
            }
        }
    }
}


// Dispatcher for zxdg_output_manager_v1
impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _manager: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        event: zxdg_output_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // zxdg_output_manager_v1 has no events for the client to handle
        log::trace!("zxdg_output_manager_v1 event: {:?}", event);
    }
}

// Dispatcher for zxdg_output_v1
impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for AppState {
    fn event(
        state: &mut AppState,
        xdg_output: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        let output_entry = state.outputs.iter_mut().find(|(_, _, xdg_opt, _)| xdg_opt.as_ref().map_or(false, |x| x == xdg_output));

        if let Some((_, _, _, info)) = output_entry {
            match event {
                zxdg_output_v1::Event::LogicalPosition { x, y } => {
                    log::debug!("zxdg_output_v1 ({:?}) LogicalPosition: x={}, y={}", xdg_output.id(), x, y);
                }
                zxdg_output_v1::Event::LogicalSize { width, height } => {
                    log::debug!("zxdg_output_v1 ({:?}) LogicalSize: width={}, height={}", xdg_output.id(), width, height);
                    info.logical_width = width;
                    info.logical_height = height;
                }
                zxdg_output_v1::Event::Done => {
                    log::debug!("zxdg_output_v1 ({:?}) Done. Current info: {:?}", xdg_output.id(), info);
                    // Here, all info for this output should be collected.
                    // We might want to trigger a redraw or re-evaluation of overlay size/position if it depends on this.
                }
                zxdg_output_v1::Event::Name { name } => {
                    log::info!("zxdg_output_v1 ({:?}) Name: {}", xdg_output.id(), name);
                    info.name = Some(name);
                }
                zxdg_output_v1::Event::Description { description } => {
                    log::info!("zxdg_output_v1 ({:?}) Description: {}", xdg_output.id(), description);
                    info.description = Some(description);
                }
                _ => {
                     log::trace!("Unhandled zxdg_output_v1 event: {:?}", event);
                }
            }
        } else {
            log::warn!("Received zxdg_output_v1 event for an unknown output object: {:?}", xdg_output.id());
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

// Dispatcher for zwlr_layer_shell_v1
impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for AppState {
    fn event(
        _state: &mut AppState,
        _proxy: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        event: zwlr_layer_shell_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
    ) {
        // zwlr_layer_shell_v1 has no events for the client to handle
        log::trace!("zwlr_layer_shell_v1 event: {:?}", event);
    }
}

/// Command-line arguments
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Check the configuration file for errors and print layout information
    #[clap(long)]
    check: bool,

    /// Path to the configuration file
    #[clap(long, value_parser, default_value = "keys.toml")]
    config: String,

    /// Run in overlay mode (requires compositor support for wlr-layer-shell)
    #[clap(long)]
    overlay: bool,
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

fn print_overlay_config_for_check(config: &OverlayConfig) {
    println!("\nOverlay Configuration:");
    println!("  Screen:               {}", config.screen.as_deref().unwrap_or("Compositor default"));
    println!("  Position:             {:?}", config.position);

    let width_str = match config.size_width {
        Some(SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from height/layout".to_string(),
    };
    let height_str = match config.size_height {
        Some(SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from width/layout".to_string(),
    };
    println!("  Size Width:           {}", width_str);
    println!("  Size Height:          {}", height_str);

    println!("  Margins (T,R,B,L):    {}, {}, {}, {}", config.margin_top, config.margin_right, config.margin_bottom, config.margin_left);

    match parse_color_string(&config.background_color_inactive) {
        Ok((r,g,b,a)) => println!("  Background Inactive:  {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2})", config.background_color_inactive, r,g,b,a),
        Err(e) => println!("  Background Inactive:  {} (Error: {})", config.background_color_inactive, e),
    }
    match parse_color_string(&config.background_color_active) {
        Ok((r,g,b,a)) => println!("  Background Active:    {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2}) (currently unused for global bg)", config.background_color_active, r,g,b,a),
        Err(e) => println!("  Background Active:    {} (Error: {})", config.background_color_active, e),
    }
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
        println!("{:<20} | {:<25} | {:<10} | {:<10} | {:<20}",
                 "Label (Name)", "Bounding Box (L,T,R,B)", "Keycode", "Font Scale", "Truncated Label");
        println!("{:-<20}-+-{:-<25}-+-{:-<10}-+-{:-<10}-+-{:-<20}", "", "", "", "", "");

        for key_config in &app_config.key {
            let right_edge = key_config.left + key_config.width;
            let bottom_edge = key_config.top + key_config.height;
            let bbox_str = format!("{:.1},{:.1}, {:.1},{:.1}",
                                   key_config.left, key_config.top,
                                   right_edge, bottom_edge);

            let initial_font_size = key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64;

            match simulate_text_layout(key_config, &ft_face) {
                Ok(text_check_result) => {
                    let font_scale = if initial_font_size > 0.0 {
                        text_check_result.final_font_size_pts / initial_font_size
                    } else {
                        1.0 // Avoid division by zero, should not happen with validation
                    };

                    let truncated_label_display = if text_check_result.truncated_chars > 0 || !text_check_result.final_text.eq(&key_config.name) {
                        text_check_result.final_text
                    } else {
                        "".to_string() // Empty if not truncated from original name
                    };

                    println!("{:<20} | {:<25} | {:<10} | {:<10.2} | {:<20}",
                             key_config.name,
                             bbox_str,
                             key_config.keycode,
                             font_scale,
                             truncated_label_display
                        );
                }
                Err(e) => {
                     println!("{:<20} | {:<25} | {:<10} | {:<10.2} | Error simulating text: {} ",
                             key_config.name,
                             bbox_str,
                             key_config.keycode,
                             1.0, // Default scale in case of error
                             e
                        );
                }
            }
        }

        print_overlay_config_for_check(&app_config.overlay);

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

    if cli.overlay {
        log::info!("Overlay mode requested. Attempting to use wlr-layer-shell.");
        if let Some(layer_shell) = app_state.layer_shell.as_ref() {
            let mut selected_wl_output: Option<&wl_output::WlOutput> = None;

            if let Some(target_screen_specifier) = app_state.config.overlay.screen.as_ref() {
                log::info!("Attempting to find screen specified as: '{}'", target_screen_specifier);
                // Try to parse as index first
                if let Ok(target_idx) = target_screen_specifier.parse::<usize>() {
                    if let Some((_, wl_output, _, info)) = app_state.outputs.get(target_idx) {
                        selected_wl_output = Some(wl_output);
                        log::info!("Selected screen by index {}: {:?}", target_idx, info.name.as_deref().unwrap_or("N/A"));
                    } else {
                        log::warn!("Screen index {} out of bounds ({} outputs available). Compositor will choose.", target_idx, app_state.outputs.len());
                    }
                } else {
                    // Try to match by name (zxdg_output_v1 name or description)
                    let mut found = false;
                    for (_, wl_output, _, info) in &app_state.outputs {
                        if info.name.as_deref() == Some(target_screen_specifier) || info.description.as_deref() == Some(target_screen_specifier) {
                            selected_wl_output = Some(wl_output);
                            log::info!("Selected screen by name/description '{}': {:?}", target_screen_specifier, info.name.as_deref().unwrap_or("N/A"));
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        log::warn!("Screen specifier '{}' not found by name or description. Compositor will choose.", target_screen_specifier);
                        log::debug!("Available outputs for matching by name/description:");
                        for (idx, (_, _, _, info)) in app_state.outputs.iter().enumerate() {
                            log::debug!("  [{}]: Name: {:?}, Description: {:?}", idx, info.name, info.description);
                        }
                    }
                }
            } else {
                log::info!("No specific screen configured. Compositor will choose.");
            }

            let layer_surface_obj = layer_shell.get_layer_surface(
                &surface,
                selected_wl_output, // output: None means compositor chooses
                zwlr_layer_shell_v1::Layer::Overlay,
                "wayland-kbd-osd".to_string(), // namespace
                &qh,
                ()
            );
            // Configure the layer surface
            let anchor = match app_state.config.overlay.position {
                OverlayPosition::Top => zwlr_layer_surface_v1::Anchor::Top,
                OverlayPosition::Bottom => zwlr_layer_surface_v1::Anchor::Bottom,
                OverlayPosition::Left => zwlr_layer_surface_v1::Anchor::Left,
                OverlayPosition::Right => zwlr_layer_surface_v1::Anchor::Right,
                OverlayPosition::Center => zwlr_layer_surface_v1::Anchor::empty(), // Centered by default if no anchor
                OverlayPosition::TopLeft => zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left,
                OverlayPosition::TopCenter => zwlr_layer_surface_v1::Anchor::Top, // Rely on centering for horizontal
                OverlayPosition::TopRight => zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Right,
                OverlayPosition::BottomLeft => zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Left,
                OverlayPosition::BottomCenter => zwlr_layer_surface_v1::Anchor::Bottom, // Rely on centering for horizontal
                OverlayPosition::BottomRight => zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Right,
                OverlayPosition::CenterLeft => zwlr_layer_surface_v1::Anchor::Left, // Rely on centering for vertical
                OverlayPosition::CenterRight => zwlr_layer_surface_v1::Anchor::Right, // Rely on centering for vertical
            };
            log::info!("Setting anchor to: {:?}", anchor);
            layer_surface_obj.set_anchor(anchor);

            let margins = &app_state.config.overlay;
            log::info!("Setting margins: T={}, R={}, B={}, L={}", margins.margin_top, margins.margin_right, margins.margin_bottom, margins.margin_left);
            layer_surface_obj.set_margin(margins.margin_top, margins.margin_right, margins.margin_bottom, margins.margin_left);

            layer_surface_obj.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
            layer_surface_obj.set_exclusive_zone(0); // Do not reserve space

            // Determine target screen dimensions for size calculation
            let mut screen_width_px = WINDOW_WIDTH; // Default if no output info
            let mut screen_height_px = WINDOW_HEIGHT; // Default if no output info

            if let Some(wl_output) = selected_wl_output {
                if let Some((_, _, _, info)) = app_state.outputs.iter().find(|(_, o, _, _)| o == wl_output) {
                    if info.logical_width > 0 && info.logical_height > 0 {
                        screen_width_px = info.logical_width;
                        screen_height_px = info.logical_height;
                        log::info!("Using selected screen dimensions for size calculation: {}x{}", screen_width_px, screen_height_px);
                    } else {
                        log::warn!("Selected screen {:?} has no logical dimensions yet. Falling back to default {}x{} for size calculation.", info.name, screen_width_px, screen_height_px);
                    }
                }
            } else if let Some((_, _, _, info)) = app_state.outputs.first() { // Default to first output if none selected
                if info.logical_width > 0 && info.logical_height > 0 {
                    screen_width_px = info.logical_width;
                    screen_height_px = info.logical_height;
                    log::info!("Using first available screen dimensions for size calculation: {}x{}", screen_width_px, screen_height_px);
                } else {
                     log::warn!("First available screen {:?} has no logical dimensions yet. Falling back to default {}x{} for size calculation.", info.name, screen_width_px, screen_height_px);
                }
            } else {
                log::warn!("No screens found/selected. Using default window size {}x{} for overlay size calculation.", screen_width_px, screen_height_px);
            }

            let (layout_bound_w, layout_bound_h) = app_state.get_key_layout_bounds();
            let layout_aspect_ratio = if layout_bound_h > 0.0 { layout_bound_w / layout_bound_h } else { 16.0/9.0 }; // Default aspect if no keys

            let mut target_w: u32 = 0; // 0 means compositor decides or derive from other dimension
            let mut target_h: u32 = 0;

            match app_state.config.overlay.size_width {
                Some(SizeDimension::Pixels(px)) => target_w = px,
                Some(SizeDimension::Ratio(r)) => target_w = (screen_width_px as f32 * r).round() as u32,
                None => {}
            }
            match app_state.config.overlay.size_height {
                Some(SizeDimension::Pixels(px)) => target_h = px,
                Some(SizeDimension::Ratio(r)) => target_h = (screen_height_px as f32 * r).round() as u32,
                None => {}
            }

            // Preserve aspect ratio if one dimension is zero (or not specified)
            if target_w > 0 && target_h == 0 {
                if layout_aspect_ratio > 0.0 {
                    target_h = (target_w as f32 / layout_aspect_ratio).round() as u32;
                } else { // Should not happen if layout_aspect_ratio has a default
                    target_h = (target_w as f32 * (9.0/16.0)).round() as u32; // Default to 16:9 portraitish if layout is zero area
                }
            } else if target_h > 0 && target_w == 0 {
                if layout_aspect_ratio > 0.0 {
                    target_w = (target_h as f32 * layout_aspect_ratio).round() as u32;
                } else {
                     target_w = (target_h as f32 * (16.0/9.0)).round() as u32; // Default to 16:9 landscapeish
                }
            } else if target_w == 0 && target_h == 0 {
                 // This case means neither width nor height was specified in config.
                 // The default for size_height is Ratio(0.3), so this block might not be hit often
                 // unless defaults are removed or both are explicitly set to null/None in TOML.
                 // If compositor also suggests 0,0, it could be an issue.
                 // For now, let layer_surface.set_size(0,0) pass through.
                 // Compositor might give a default size or size based on content (which is tricky here).
                 log::warn!("Overlay width and height are both zero. Compositor will determine size. This might be very small.");
            }


            // Ensure calculated size does not exceed screen dimensions, preserving aspect ratio.
            // This is a "max size" constraint.
            if target_w > screen_width_px as u32 && screen_width_px > 0 {
                let original_target_w = target_w;
                target_w = screen_width_px as u32;
                if layout_aspect_ratio > 0.0 { // Avoid division by zero
                    target_h = (target_w as f32 / layout_aspect_ratio).round() as u32;
                }
                 log::info!("Target width {} exceeded screen width {}. Adjusted to {}x{} to fit screen width.", original_target_w, screen_width_px, target_w, target_h);
            }
            if target_h > screen_height_px as u32 && screen_height_px > 0 {
                let original_target_h = target_h;
                target_h = screen_height_px as u32;
                // If width was also capped, this might shrink it further.
                // If width was not capped, or if this cap is more restrictive:
                if layout_aspect_ratio > 0.0 { // Avoid division by zero
                     target_w = (target_h as f32 * layout_aspect_ratio).round() as u32;
                }
                log::info!("Target height {} exceeded screen height {}. Adjusted to {}x{} to fit screen height.", original_target_h, screen_height_px, target_w, target_h);
            }

            // Final safety check for zero dimensions if layout was empty.
            // If target_w or target_h is still 0, layer-shell expects this to mean "derive from anchor".
            // E.g. anchor left+right and width 0 means full width.
            // If no anchors in a dimension, 0 means "as small as possible" or compositor default.
            // We want a defined size if possible.
            if target_w == 0 && target_h == 0 && (layout_bound_w > 0.0 || layout_bound_h > 0.0) {
                // This implies config asked for 0x0, but we have a layout.
                // Fallback to a small portion of screen, e.g., 30% height and derive width.
                target_h = (screen_height_px as f32 * 0.3).round() as u32;
                if layout_aspect_ratio > 0.0 {
                    target_w = (target_h as f32 * layout_aspect_ratio).round() as u32;
                } else {
                    target_w = (screen_width_px as f32 * 0.5).round() as u32; // Fallback width
                }
                log::warn!("Overlay size was 0x0 despite having a layout. Defaulting to {}x{}.", target_w, target_h);
            }


            log::info!("Setting layer surface size to: {}x{}", target_w, target_h);
            layer_surface_obj.set_size(target_w, target_h);

            // The app_state.configured_width/height will be updated by the compositor via ::Configure event.
            // We pass our desired size to layer_surface.set_size(). The compositor might adjust it.
            // The drawing logic uses app_state.configured_width/height.

            app_state.layer_surface = Some(layer_surface_obj);
            log::info!("Created and configured layer surface for overlay mode.");
        } else {
            log::error!("--overlay flag was used, but zwlr_layer_shell_v1 is not available from the compositor. Falling back to normal window mode.");
            // Fallback to XDG toplevel
            let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
            let toplevel = xdg_surface.get_toplevel(&qh, ());
            toplevel.set_title("Wayland Keyboard OSD (Fallback)".to_string());
            app_state.surface = Some(surface.clone()); // Ensure surface is set for XDG path
        }
    } else {
        log::info!("Normal window mode requested (XDG shell).");
        let xdg_surface = app_state.xdg_wm_base.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());
        toplevel.set_title("Wayland Keyboard OSD".to_string());
    }

    surface.commit(); // Commit to make the surface known to the compositor and apply layer/xdg settings

    // Dispatch events once to process initial configure and draw the window.
    log::info!("Initial surface commit done. Dispatching events to catch initial configure...");
    if event_queue.roundtrip(&mut app_state).is_err() {
        log::error!("Error during roundtrip after surface commit (waiting for initial configure).");
        // Depending on the error, might want to exit or handle differently
    }
    // An explicit draw call here might be needed if the first configure doesn't trigger it,
    // or if we want to show something before the first configure.
    // However, the xdg_surface configure event should trigger the first draw.
    log::info!("Initial roundtrip complete. Wayland window should be configured. Waiting for events...");

    // Imports for LibinputEvent, KeyboardEvent, KeyState, KeyboardEventTrait, WaylandError
    // are now at the top of the module.
    use std::os::unix::io::{AsRawFd as _, RawFd};
    use std::io;

    log::info!("Entering main event loop.");

    let wayland_raw_fd: RawFd = match conn.prepare_read() {
        Ok(guard) => guard.connection_fd().as_raw_fd(),
        Err(e) => {
            log::error!("Failed to prepare_read Wayland connection before starting event loop: {}", e);
            process::exit(1);
        }
    };

    let mut fds: Vec<libc::pollfd> = Vec::new();
    fds.push(libc::pollfd { fd: wayland_raw_fd, events: libc::POLLIN, revents: 0 });
    const WAYLAND_FD_IDX: usize = 0;

    let mut libinput_fd_idx_opt: Option<usize> = None;
    if let Some(ref context) = app_state.input_context { // context can be an immutable ref now
        let libinput_raw_fd: RawFd = context.as_raw_fd();
        fds.push(libc::pollfd { fd: libinput_raw_fd, events: libc::POLLIN, revents: 0 });
        libinput_fd_idx_opt = Some(fds.len() - 1);
        log_if_input_device_access_denied(&app_state.config, true); // Pass true since context is Some
    } else {
        log_if_input_device_access_denied(&app_state.config, false); // Pass false since context is None
        log::warn!("No libinput context available. Key press/release events will not be monitored.");
    }


    let poll_timeout_ms = 33; // Timeout for poll in milliseconds (previously 100)

    while app_state.running {
        for item in fds.iter_mut() { item.revents = 0; }
        // `needs_redraw` is now part of `app_state` and is managed across iterations/event types.

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
                if handle_wayland_events(&conn, &mut event_queue, &mut app_state).is_err() {
                    // app_state.running is set to false within handle_wayland_events on error
                    break;
                }
            }
            if (fds[WAYLAND_FD_IDX].revents & (libc::POLLERR | libc::POLLHUP)) != 0 {
                log::error!("Wayland FD error/hangup (POLLERR/POLLHUP). Exiting.");
                app_state.running = false;
            }

            // Libinput events
            if let Some(libinput_idx) = libinput_fd_idx_opt {
                if app_state.running && (fds[libinput_idx].revents & libc::POLLIN) != 0 {
                    handle_libinput_events(&mut app_state);
                }
                if app_state.running && (fds[libinput_idx].revents & (libc::POLLERR | libc::POLLHUP)) != 0 {
                    log::error!("Libinput FD error/hangup (POLLERR/POLLHUP). Input monitoring might stop.");
                    // Attempt to dispatch any remaining events from libinput before removing it
                    if let Some(ref mut context) = app_state.input_context {
                        let _ = context.dispatch(); // Ignore error here, as we are already in an error state
                    }
                    app_state.input_context = None; // Stop using libinput

                    // Remove the libinput FD from the poll list carefully
                    // Find the actual index of libinput_fd in fds, in case WAYLAND_FD_IDX is not 0 or list changes
                    if let Some(idx_to_remove) = fds.iter().position(|pollfd| pollfd.fd == fds[libinput_idx].fd) {
                        fds.remove(idx_to_remove);
                    }
                    libinput_fd_idx_opt = None; // Clear the option

                    log::warn!("Libinput context removed due to FD error. Key press/release events will no longer be monitored.");
                }
            }
        }

        if !app_state.running {
            break;
        }

        if app_state.needs_redraw {
            if app_state.surface.is_some() {
                log::debug!("Main loop: needs_redraw is true, calling draw.");
                app_state.draw(&qh);
                app_state.needs_redraw = false; // Reset after drawing
            } else {
                log::warn!("Main loop: needs_redraw is true, but surface is None. Skipping draw.");
                app_state.needs_redraw = false; // Still reset to prevent loop if surface never appears
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

fn handle_wayland_events(
    conn: &Connection,
    event_queue: &mut wayland_client::EventQueue<AppState>,
    app_state: &mut AppState,
) -> Result<(), ()> {
    match conn.prepare_read() { // Changed from if let Some(guard)
        Ok(guard) => { // Handle Ok case
            match guard.read() {
                Ok(bytes_read) => {
                    log::trace!("Successfully read {} bytes from Wayland socket", bytes_read);
                    match event_queue.dispatch_pending(app_state) {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("Error dispatching Wayland events: {}", e);
                            app_state.running = false;
                            return Err(());
                        }
                    }
                }
                Err(WaylandError::Io(io_err)) if io_err.kind() == std::io::ErrorKind::WouldBlock => {
                    // No new events, this is normal.
                }
                Err(e) => {
                    log::error!("Error reading from Wayland connection: {}", e);
                    app_state.running = false;
                    return Err(());
                }
            }
        }
        Err(e) => { // Handle Err case from prepare_read
            log::error!("Failed to prepare_read Wayland connection in event handler: {}", e);
            app_state.running = false;
            return Err(());
        }
    }
    Ok(())
}

fn handle_libinput_events(app_state: &mut AppState) {
    if let Some(ref mut context) = app_state.input_context {
        if context.dispatch().is_err() {
            log::error!("Libinput dispatch error");
        }
        while let Some(event) = context.next() {
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

// Helper to log if input devices are inaccessible after attempting to assign seat
// This is a common issue and providing a hint can be useful.
// This function is called after `udev_assign_seat` has already been attempted.
fn log_if_input_device_access_denied(app_config: &AppConfig, input_context_is_some: bool) {
    // This function serves as a general reminder if input is configured but might not be working,
    // prompting the user to check earlier, more specific error logs.
    if !app_config.key.is_empty() && input_context_is_some {
        // If we have an input_context and keys are configured, remind to check for device open errors.
        log::info!(
            "Libinput context was initialized. If keys do not respond, please check previous log messages \
            for any 'Failed to open path' errors from the input system. These errors often indicate \
            permission issues (e.g., the user running the application may not be in the 'input' group)."
        );
    } else if !app_config.key.is_empty() && !input_context_is_some {
        // If keys are configured but there's no input_context, it means the initial udev_assign_seat likely failed.
        // The error for that failure would have been logged already by the caller.
        log::warn!(
            "Key input is configured in keys.toml, but the libinput context could not be initialized \
            (see previous errors). Key press/release events will not be monitored."
        );
    }
    // If app_config.key is empty, no warning is needed as no input is expected.
}
