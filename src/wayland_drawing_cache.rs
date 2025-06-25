// src/wayland_drawing_cache.rs

#[derive(Debug, Clone)]
pub struct DrawingCache {
    pub last_draw_width: i32,
    pub last_draw_height: i32,
    pub cached_scale: f32,
    pub cached_offset_x: f32,
    pub cached_offset_y: f32,
    pub layout_cache_valid: bool,
}

impl Default for DrawingCache {
    fn default() -> Self {
        DrawingCache {
            last_draw_width: 0,
            last_draw_height: 0,
            cached_scale: 1.0,
            cached_offset_x: 0.0,
            cached_offset_y: 0.0,
            layout_cache_valid: false,
        }
    }
}

impl DrawingCache {
    pub fn invalidate(&mut self) {
        self.layout_cache_valid = false;
        // Optionally reset other fields if needed when invalidated,
        // but layout_cache_valid = false should trigger recalculation.
    }

    pub fn update(&mut self, width: i32, height: i32, scale: f32, offset_x: f32, offset_y: f32) {
        self.last_draw_width = width;
        self.last_draw_height = height;
        self.cached_scale = scale;
        self.cached_offset_x = offset_x;
        self.cached_offset_y = offset_y;
        self.layout_cache_valid = true;
    }

    pub fn is_valid_for_dimensions(&self, current_width: i32, current_height: i32) -> bool {
        self.layout_cache_valid && self.last_draw_width == current_width && self.last_draw_height == current_height
    }
}
