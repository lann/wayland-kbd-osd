// Wayland interaction
// Added for input::Libinput
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use memmap2::MmapMut;
use std::collections::HashMap;
use std::fs::File as StdFsFile;
use std::os::unix::io::AsRawFd;

use crate::config::{
    default_key_background_color_string, parse_color_string, AppConfig, SizeDimension,
    DEFAULT_BORDER_THICKNESS_UNSCALED, DEFAULT_CORNER_RADIUS_UNSCALED, DEFAULT_ROTATION_DEGREES,
    DEFAULT_TEXT_SIZE_UNSCALED,
};
use crate::draw::{self, KeyDisplay}; // Import draw module and KeyDisplay

// Graphics and Font rendering (needed for font loading in AppState::draw)
use cairo::FontFace as CairoFontFace;
use freetype::Library as FreeTypeLibrary;

pub const WINDOW_WIDTH: i32 = 320;
pub const WINDOW_HEIGHT: i32 = 240;

#[derive(Debug, Clone, Default)]
pub struct OutputInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub logical_width: i32,
    pub logical_height: i32,
}

pub struct AppState {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub xdg_output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    pub outputs: Vec<(
        u32,
        wl_output::WlOutput,
        Option<zxdg_output_v1::ZxdgOutputV1>,
        OutputInfo,
    )>,
    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    pub buffer: Option<wl_buffer::WlBuffer>,
    pub mmap: Option<MmapMut>,
    pub temp_file: Option<StdFsFile>,
    pub configured_width: i32,
    pub configured_height: i32,
    pub running: bool,
    pub input_context: Option<input::Libinput>,
    pub config: AppConfig,
    pub key_states: HashMap<u32, bool>,
    pub needs_redraw: bool,
    pub last_draw_width: i32,
    pub last_draw_height: i32,
    pub cached_scale: f32,
    pub cached_offset_x: f32,
    pub cached_offset_y: f32,
    pub layout_cache_valid: bool,
    pub initial_surface_size_set: bool,
    pub target_output_identifier: Option<String>,
    pub identified_target_wl_output_name: Option<u32>,
    pub frame_callback: Option<wl_callback::WlCallback>,
}

impl AppState {
    pub fn new(app_config: AppConfig) -> Self {
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
            config: app_config.clone(),
            key_states: key_states_map,
            needs_redraw: true,
            last_draw_width: 0,
            last_draw_height: 0,
            cached_scale: 1.0,
            cached_offset_x: 0.0,
            cached_offset_y: 0.0,
            layout_cache_valid: false,
            initial_surface_size_set: false,
            target_output_identifier: app_config.overlay.screen.clone(),
            identified_target_wl_output_name: None,
            frame_callback: None,
        }
    }

    pub fn attempt_configure_layer_surface_size(&mut self) {
        if self.initial_surface_size_set || self.layer_surface.is_none() || self.surface.is_none() {
            return;
        }
        let mut screen_width_px = WINDOW_WIDTH;
        let mut screen_height_px = WINDOW_HEIGHT;
        let mut found_target_output_dimensions = false;
        if let Some(target_wl_name) = self.identified_target_wl_output_name {
            if let Some((_, _, _, info)) = self
                .outputs
                .iter()
                .find(|(name, _, _, _)| *name == target_wl_name)
            {
                if info.logical_width > 0 && info.logical_height > 0 {
                    screen_width_px = info.logical_width;
                    screen_height_px = info.logical_height;
                    found_target_output_dimensions = true;
                    log::info!(
                        "Using dimensions from targeted output (ID: {} Name: {:?}): {}x{}",
                        target_wl_name,
                        info.name.as_deref().unwrap_or("N/A"),
                        screen_width_px,
                        screen_height_px
                    );
                } else {
                    log::warn!(
                        "Targeted output (ID: {}) has invalid dimensions ({}x{}). Waiting.",
                        target_wl_name,
                        info.logical_width,
                        info.logical_height
                    );
                    return;
                }
            } else {
                log::warn!(
                    "Previously identified target output (ID: {}) no longer found.",
                    target_wl_name
                );
            }
        }
        if !found_target_output_dimensions {
            if let Some((id, _, _, info)) = self
                .outputs
                .iter()
                .find(|(_, _, _, i)| i.logical_width > 0 && i.logical_height > 0)
            {
                screen_width_px = info.logical_width;
                screen_height_px = info.logical_height;
                log::info!(
                    "Using first available screen (ID: {}, Name: {:?}): {}x{}",
                    id,
                    info.name.as_deref().unwrap_or("N/A"),
                    screen_width_px,
                    screen_height_px
                );
                if self.identified_target_wl_output_name.is_none() {
                    self.identified_target_wl_output_name = Some(*id);
                }
            } else {
                log::warn!(
                    "No screen with valid dimensions. Falling back to {}x{} for size calculation.",
                    screen_width_px,
                    screen_height_px
                );
            }
        }
        let (layout_w, layout_h) = self.get_key_layout_bounds();
        let aspect = if layout_h > 0.0 {
            layout_w / layout_h
        } else {
            16.0 / 9.0
        };
        let mut target_w = 0;
        let mut target_h = 0;
        match self.config.overlay.size_width {
            Some(SizeDimension::Pixels(px)) => target_w = px,
            Some(SizeDimension::Ratio(r)) => target_w = (screen_width_px as f32 * r).round() as u32,
            None => {}
        }
        match self.config.overlay.size_height {
            Some(SizeDimension::Pixels(px)) => target_h = px,
            Some(SizeDimension::Ratio(r)) => {
                target_h = (screen_height_px as f32 * r).round() as u32
            }
            None => {}
        }
        if target_w > 0 && target_h == 0 {
            target_h = (target_w as f32 / aspect).round() as u32;
        } else if target_h > 0 && target_w == 0 {
            target_w = (target_h as f32 * aspect).round() as u32;
        } else if target_w == 0 && target_h == 0 {
            target_h = (screen_height_px as f32 * 0.3).round() as u32;
            target_w = (target_h as f32 * aspect).round() as u32;
            log::warn!("Overlay size 0x0. Defaulting: {}x{}", target_w, target_h);
        }
        if target_w > screen_width_px as u32 && screen_width_px > 0 {
            let old_w = target_w;
            target_w = screen_width_px as u32;
            target_h = (target_w as f32 / aspect).round() as u32;
            log::info!(
                "Width {} exceeded screen {}. Adjusted: {}x{}",
                old_w,
                screen_width_px,
                target_w,
                target_h
            );
        }
        if target_h > screen_height_px as u32 && screen_height_px > 0 {
            let old_h = target_h;
            target_h = screen_height_px as u32;
            target_w = (target_h as f32 * aspect).round() as u32;
            log::info!(
                "Height {} exceeded screen {}. Adjusted: {}x{}",
                old_h,
                screen_height_px,
                target_w,
                target_h
            );
        }
        if target_w == 0 && screen_width_px > 0 {
            target_w = (screen_width_px as f32 * 0.1).round().max(1.0) as u32;
        }
        if target_h == 0 && screen_height_px > 0 {
            target_h = (screen_height_px as f32 * 0.1).round().max(1.0) as u32;
        }
        if target_w == 0 {
            target_w = 100;
        }
        if target_h == 0 {
            target_h = 50;
        }
        log::info!("Setting layer surface size: {}x{}", target_w, target_h);
        if let Some(ls) = self.layer_surface.as_ref() {
            ls.set_size(target_w, target_h);
            self.initial_surface_size_set = true;
            if let Some(s) = self.surface.as_ref() {
                s.commit();
                log::info!("Layer surface size set, surface committed.");
            } else {
                log::error!("Cannot commit surface for layer size, surface is None.");
            }
            self.needs_redraw = true;
        } else {
            log::error!("Cannot set layer surface size, layer_surface is None.");
        }
    }

    pub fn get_key_layout_bounds(&self) -> (f32, f32) {
        if self.config.key.is_empty() {
            return (0.0, 0.0);
        }
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for kc in &self.config.key {
            min_x = min_x.min(kc.left);
            max_x = max_x.max(kc.left + kc.width);
            min_y = min_y.min(kc.top);
            max_y = max_y.max(kc.top + kc.height);
        }
        ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
    }

    pub fn draw(&mut self, qh: &QueueHandle<AppState>) {
        if self.surface.is_none() || self.shm.is_none() || self.compositor.is_none() {
            log::error!("Draw: missing Wayland objects.");
            return;
        }
        let surface = self.surface.as_ref().unwrap();
        let shm = self.shm.as_ref().unwrap();
        let width = self.configured_width;
        let height = self.configured_height;
        let stride = width * 4;
        let size = (stride * height) as usize;
        if self.temp_file.is_none()
            || self.mmap.is_none()
            || self.mmap.as_ref().is_none_or(|m| m.len() < size)
        {
            if let Some(b) = self.buffer.take() {
                b.destroy();
            }
            self.mmap = None;
            self.temp_file = None;
            let tf = tempfile::tempfile().expect("SHM tempfile creation failed");
            tf.set_len(size as u64)
                .expect("SHM tempfile set_len failed");
            self.mmap = Some(unsafe { MmapMut::map_mut(&tf).expect("SHM mmap failed") });
            self.temp_file = Some(tf);
        }
        let fd = self.temp_file.as_ref().unwrap().as_raw_fd();
        let pool = shm.create_pool(fd, size as i32, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();
        let mmap_ptr = self.mmap.as_mut().unwrap().as_mut_ptr();
        let cairo_surface = match unsafe {
            cairo::ImageSurface::create_for_data_unsafe(
                mmap_ptr,
                cairo::Format::ARgb32,
                width,
                height,
                stride,
            )
        } {
            Ok(s) => s,
            Err(e) => {
                log::error!("Cairo ImageSurface creation failed: {:?}", e);
                surface.attach(Some(&buffer), 0, 0);
                surface.damage_buffer(0, 0, width, height);
                surface.commit();
                if let Some(ob) = self.buffer.replace(buffer) {
                    ob.destroy();
                }
                return;
            }
        };
        let ctx = cairo::Context::new(&cairo_surface).expect("Cairo Context creation failed");

        // --- Start of logic moved from draw::render_keyboard_to_context ---
        let scale: f32;
        let offset_x: f32;
        let offset_y: f32;
        if self.layout_cache_valid
            && self.configured_width == self.last_draw_width
            && self.configured_height == self.last_draw_height
        {
            scale = self.cached_scale;
            offset_x = self.cached_offset_x;
            offset_y = self.cached_offset_y;
        } else {
            let (layout_w, layout_h) = self.get_key_layout_bounds(); // Use existing method
            let padding = if self.config.overlay.size_width.is_some()
                || self.config.overlay.size_height.is_some()
            {
                2.0
            } else {
                (width.min(height) as f32 * 0.05).max(5.0)
            };
            let draw_w = (width as f32 - 2.0 * padding).max(0.0);
            let draw_h = (height as f32 - 2.0 * padding).max(0.0);
            let scale_x = if layout_w > 0.0 {
                draw_w / layout_w
            } else {
                1.0
            };
            let scale_y = if layout_h > 0.0 {
                draw_h / layout_h
            } else {
                1.0
            };
            scale = scale_x.min(scale_y).max(0.01);
            let scaled_layout_w = layout_w * scale;
            let scaled_layout_h = layout_h * scale;
            let min_coord_x = self
                .config
                .key
                .iter()
                .map(|k| k.left)
                .fold(f32::MAX, |a, b| a.min(b)); // Recalc min_coord for offset
            let min_coord_y = self
                .config
                .key
                .iter()
                .map(|k| k.top)
                .fold(f32::MAX, |a, b| a.min(b));
            offset_x = padding + (draw_w - scaled_layout_w) / 2.0 - (min_coord_x * scale);
            offset_y = padding + (draw_h - scaled_layout_h) / 2.0 - (min_coord_y * scale);
            self.cached_scale = scale;
            self.cached_offset_x = offset_x;
            self.cached_offset_y = offset_y;
            self.last_draw_width = width;
            self.last_draw_height = height;
            self.layout_cache_valid = true;
        }

        let font_data: &[u8] = include_bytes!("../default-font/DejaVuSansMono.ttf");
        let ft_library = FreeTypeLibrary::init().expect("FT init failed");
        let ft_face = ft_library
            .new_memory_face(font_data.to_vec(), 0)
            .expect("FT face load failed");
        let cairo_font_face =
            CairoFontFace::create_from_ft(&ft_face).expect("Cairo FT face creation failed");

        let default_fallback_color = (0.1, 0.1, 0.1, 1.0);
        let key_outline_color = parse_color_string(&self.config.overlay.default_key_outline_color)
            .unwrap_or(default_fallback_color);
        let default_key_text_color =
            parse_color_string(&self.config.overlay.default_key_text_color)
                .unwrap_or(default_fallback_color);
        let active_key_bg_color =
            parse_color_string(&self.config.overlay.active_key_background_color)
                .unwrap_or((0.6, 0.6, 0.9, 1.0));
        let active_key_text_color = parse_color_string(&self.config.overlay.active_key_text_color)
            .unwrap_or(default_key_text_color);
        let ultimate_inactive_bg_fallback =
            parse_color_string(&default_key_background_color_string())
                .unwrap_or((0.3, 0.3, 0.3, 0.5));

        let keys_to_draw: Vec<KeyDisplay> = self
            .config
            .key
            .iter()
            .map(|kc| {
                let is_pressed = *self.key_states.get(&kc.keycode).unwrap_or(&false);
                let bg_color = if is_pressed {
                    active_key_bg_color
                } else {
                    kc.background_color
                        .as_ref()
                        .and_then(|s| parse_color_string(s).ok())
                        .unwrap_or_else(|| {
                            parse_color_string(&self.config.overlay.default_key_background_color)
                                .unwrap_or(ultimate_inactive_bg_fallback)
                        })
                };
                let text_color = if is_pressed {
                    active_key_text_color
                } else {
                    default_key_text_color
                };
                KeyDisplay {
                    text: kc.name.clone(),
                    center_x: (kc.left + kc.width / 2.0) * scale + offset_x,
                    center_y: (kc.top + kc.height / 2.0) * scale + offset_y,
                    width: kc.width * scale,
                    height: kc.height * scale,
                    corner_radius: kc.corner_radius.unwrap_or(DEFAULT_CORNER_RADIUS_UNSCALED)
                        * scale,
                    border_thickness: kc
                        .border_thickness
                        .unwrap_or(DEFAULT_BORDER_THICKNESS_UNSCALED)
                        * scale,
                    rotation_degrees: kc.rotation_degrees.unwrap_or(DEFAULT_ROTATION_DEGREES),
                    text_size: kc.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) * scale,
                    border_color: key_outline_color,
                    background_color: bg_color,
                    text_color,
                }
            })
            .collect();
        // --- End of logic moved from draw::render_keyboard_to_context ---

        draw::paint_all_keys(
            &ctx,
            &keys_to_draw,
            &self.config.overlay.background_color_inactive,
            &cairo_font_face,
        );

        cairo_surface.flush();
        log::debug!("Draw complete.");
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width, height);
        surface.commit();
        if let Some(old_buffer) = self.buffer.replace(buffer) {
            old_buffer.destroy();
        }

        // After drawing and committing, if no frame callback is already pending, request one.
        if self.frame_callback.is_none() {
            if let Some(surface_ref) = self.surface.as_ref() {
                let callback = surface_ref.frame(qh, ());
                self.frame_callback = Some(callback);
                log::trace!("Frame callback requested from AppState::draw");
            } else {
                log::warn!("AppState::draw: Cannot request frame callback, surface is None after draw logic.");
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        ls: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                log::info!("LayerSurface Configure: {}, {}x{}", serial, width, height);
                if width > 0 {
                    state.configured_width = width as i32;
                }
                if height > 0 {
                    state.configured_height = height as i32;
                }
                ls.ack_configure(serial);
                state.needs_redraw = true;
                if state.surface.is_some() {
                    state.draw(qh);
                    state.needs_redraw = false;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                log::info!("LayerSurface Closed.");
                state.running = false;
            }
            _ => {
                log::trace!("Unhandled zwlr_layer_surface_v1 event: {:?}", event);
            }
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
    fn event(
        _: &mut Self,
        p: &xdg_wm_base::XdgWmBase,
        e: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = e {
            p.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
    fn event(
        s: &mut Self,
        p: &xdg_surface::XdgSurface,
        e: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial, .. } = e {
            p.ack_configure(serial);
            s.needs_redraw = true; // Signal that a redraw is needed
            if s.surface.is_some() {
                // Calling draw here will perform the initial draw and schedule the first frame callback.
                // The needs_redraw flag will be handled by the frame callback logic thereafter.
                s.draw(qh);
            } else {
                log::warn!("XDGSurface Configure: surface is None, cannot draw or schedule frame callback yet.");
            }
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for AppState {
    fn event(
        s: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        e: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 {
                    s.configured_width = width;
                }
                if height > 0 {
                    s.configured_height = height;
                }
            }
            xdg_toplevel::Event::Close => s.running = false,
            _ => {}
        }
    }
}
impl Dispatch<wl_compositor::WlCompositor, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm::WlShm, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, 5.min(version), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1.min(version), qh, ()));
                }
                "xdg_wm_base" => {
                    state.xdg_wm_base = Some(registry.bind(name, 3.min(version), qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, 4.min(version), qh, ()));
                    log::info!("Bound zwlr_layer_shell_v1 v{}", 4.min(version));
                }
                "zxdg_output_manager_v1" => {
                    state.xdg_output_manager = Some(registry.bind(name, 3.min(version), qh, ()));
                    log::info!("Bound zxdg_output_manager_v1 v{}", 3.min(version));
                }
                "wl_output" => {
                    let out =
                        registry.bind::<wl_output::WlOutput, _, _>(name, 4.min(version), qh, ());
                    let xdg_out = state
                        .xdg_output_manager
                        .as_ref()
                        .map(|m| m.get_xdg_output(&out, qh, ()));
                    state
                        .outputs
                        .push((name, out, xdg_out, OutputInfo::default()));
                    log::info!("Bound wl_output (id {}) v{}", name, 4.min(version));
                }
                _ => {}
            }
        } else if let wl_registry::Event::GlobalRemove { name } = event {
            state.outputs.retain(|(id, _, _, _)| *id != name);
            if state.identified_target_wl_output_name == Some(name) {
                state.identified_target_wl_output_name = None;
            }
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for AppState {
    fn event(
        _s: &mut Self,
        o: &wl_output::WlOutput,
        e: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = e {
            log::debug!("wl_output {:?} Name: {}", o.id(), name);
        }
    }
}
impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        e: zxdg_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        log::trace!("zxdg_output_manager_v1 event: {:?}", e);
    }
}
impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for AppState {
    fn event(
        state: &mut Self,
        xdg_o: &zxdg_output_v1::ZxdgOutputV1,
        e: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let opt = state
            .outputs
            .iter_mut()
            .find(|(_, _, xo, _)| xo.as_ref() == Some(xdg_o));
        if let Some((id, _, _, info)) = opt {
            match e {
                zxdg_output_v1::Event::LogicalSize { width, height } => {
                    info.logical_width = width;
                    info.logical_height = height;
                    if !state.initial_surface_size_set
                        && (state.identified_target_wl_output_name.is_none()
                            || state.identified_target_wl_output_name == Some(*id))
                        && info.logical_width > 0
                        && info.logical_height > 0
                    {
                        state.attempt_configure_layer_surface_size();
                    }
                }
                zxdg_output_v1::Event::Done => {
                    if !state.initial_surface_size_set
                        && (state.identified_target_wl_output_name.is_none()
                            || state.identified_target_wl_output_name == Some(*id))
                        && info.logical_width > 0
                        && info.logical_height > 0
                    {
                        state.attempt_configure_layer_surface_size();
                    }
                }
                zxdg_output_v1::Event::Name { name } => {
                    info.name = Some(name);
                }
                zxdg_output_v1::Event::Description { description } => {
                    info.description = Some(description);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for AppState {
    fn event(
        state: &mut Self,
        callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            log::trace!("Frame callback done for callback ID: {:?}", callback.id());

            // Clear the stored callback, as it's a one-shot.
            state.frame_callback = None;

            if state.needs_redraw {
                if state.surface.is_some() {
                    log::debug!("Frame callback: needs_redraw is true, calling draw.");
                    state.draw(qh); // draw() will request a new frame callback
                    state.needs_redraw = false; // Reset after draw
                } else {
                    log::warn!("Frame callback: needs_redraw is true, but surface is None. Skipping draw.");
                    state.needs_redraw = false;
                    // Request a new frame callback even if we skipped draw, to keep the loop alive
                    // if the surface appears later and needs_redraw is set again.
                    if let Some(surface) = state.surface.as_ref() {
                        if state.frame_callback.is_none() {
                            let callback = surface.frame(qh, ());
                            state.frame_callback = Some(callback);
                            log::trace!("Frame callback requested from wl_callback::Done (surface present, draw skipped)");
                        }
                    }
                }
            } else {
                // If no redraw was needed, but a frame callback was pending,
                // we might still want to request another one if the application
                // expects to draw intermittently.
                // This ensures that if needs_redraw becomes true later,
                // a frame callback is already in flight or will be requested.
                if let Some(surface) = state.surface.as_ref() {
                    if state.frame_callback.is_none() {
                        let callback = surface.frame(qh, ());
                        state.frame_callback = Some(callback);
                        log::trace!("Frame callback requested from wl_callback::Done (no redraw needed)");
                    }
                }
            }
        } else {
            log::warn!("Received unexpected event on wl_callback: {:?}", event);
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        _: &mut Self,
        b: &wl_buffer::WlBuffer,
        e: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = e {
            log::debug!("Buffer {:?} released", b.id());
        }
    }
}
impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for AppState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        e: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        log::trace!("zwlr_layer_shell_v1 event: {:?}", e);
    }
}

pub fn handle_wayland_events(
    conn: &Connection,
    event_queue: &mut EventQueue<AppState>,
    app_state: &mut AppState,
) -> Result<(), ()> {
    match conn.prepare_read() {
        Ok(guard) => match guard.read() {
            Ok(_) => {
                if event_queue.dispatch_pending(app_state).is_err() {
                    app_state.running = false;
                    return Err(());
                }
            }
            Err(wayland_client::backend::WaylandError::Io(io_err))
                if io_err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                app_state.running = false;
                return Err(());
            }
        },
        Err(_) => {
            app_state.running = false;
            return Err(());
        }
    }
    Ok(())
}
