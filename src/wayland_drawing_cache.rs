// src/wayland_drawing_cache.rs

//! This module defines the `DrawingCache` struct, used to cache parameters
//! related to the overall layout and scaling of the keyboard OSD. This helps
//! avoid redundant calculations when the OSD dimensions or content haven't changed.

/// Caches parameters related to the overall keyboard layout and scaling.
///
/// This struct stores the last known drawing dimensions (`last_draw_width`,
/// `last_draw_height`), the calculated scale factor (`cached_scale`),
/// offsets (`cached_offset_x`, `cached_offset_y`), and a validity flag
/// (`layout_cache_valid`).
///
/// The primary purpose is to avoid recalculating the overall keyboard scale
/// and positioning if the OSD surface dimensions have not changed since the
/// last draw operation.
#[derive(Debug, Clone)]
pub struct DrawingCache {
    /// The width of the drawing area for which the cache was last updated.
    pub last_draw_width: i32,
    /// The height of the drawing area for which the cache was last updated.
    pub last_draw_height: i32,
    /// The cached scale factor applied to the entire keyboard layout.
    pub cached_scale: f32,
    /// The cached X offset for positioning the keyboard layout.
    pub cached_offset_x: f32,
    /// The cached Y offset for positioning the keyboard layout.
    pub cached_offset_y: f32,
    /// A flag indicating whether the cached layout parameters are currently valid.
    /// This is set to `false` by `invalidate()` and to `true` by `update()`.
    pub layout_cache_valid: bool,
}

impl Default for DrawingCache {
    /// Creates a default `DrawingCache` instance.
    ///
    /// The default cache is initialized with zero dimensions, a scale of 1.0,
    /// zero offsets, and is marked as invalid (`layout_cache_valid = false`).
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
    /// Invalidates the drawing cache.
    ///
    /// Sets `layout_cache_valid` to `false`, signaling that the cached
    /// parameters are stale and should be recalculated before the next draw.
    pub fn invalidate(&mut self) {
        self.layout_cache_valid = false;
        // Note: Other fields like last_draw_width/height could also be reset here,
        // but simply setting layout_cache_valid to false is sufficient to trigger
        // a recalculation by the drawing logic that uses this cache.
    }

    /// Updates the cache with new layout parameters and marks it as valid.
    ///
    /// # Arguments
    ///
    /// * `width` - The current width of the drawing area.
    /// * `height` - The current height of the drawing area.
    /// * `scale` - The new scale factor for the keyboard layout.
    /// * `offset_x` - The new X offset for the keyboard layout.
    /// * `offset_y` - The new Y offset for the keyboard layout.
    pub fn update(&mut self, width: i32, height: i32, scale: f32, offset_x: f32, offset_y: f32) {
        self.last_draw_width = width;
        self.last_draw_height = height;
        self.cached_scale = scale;
        self.cached_offset_x = offset_x;
        self.cached_offset_y = offset_y;
        self.layout_cache_valid = true;
    }

    /// Checks if the cache is currently valid for the given dimensions.
    ///
    /// # Arguments
    ///
    /// * `current_width` - The current width of the drawing area.
    /// * `current_height` - The current height of the drawing area.
    ///
    /// # Returns
    ///
    /// `true` if `layout_cache_valid` is true and `last_draw_width` and
    /// `last_draw_height` match the provided `current_width` and `current_height`.
    /// Otherwise, returns `false`.
    pub fn is_valid_for_dimensions(&self, current_width: i32, current_height: i32) -> bool {
        self.layout_cache_valid && self.last_draw_width == current_width && self.last_draw_height == current_height
    }
}
