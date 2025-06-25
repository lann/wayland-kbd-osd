// Drawing the keyboard

use crate::text_utils::{layout_text, CairoMetricsProvider, TextLayoutParams};
use cairo::{Context, FontFace as CairoFontFace};

// Struct to hold key properties for drawing (calculated from KeyConfig and AppState)
// This struct is prepared by AppState::draw and passed to paint_all_keys
#[derive(Debug)]
pub struct KeyDisplay {
    pub text: String, // Original text for the key
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
    pub border_thickness: f32,
    pub rotation_degrees: f32,
    pub text_size: f32, // Initial desired text size in points
    pub border_color: (f64, f64, f64, f64),
    pub background_color: (f64, f64, f64, f64),
    pub text_color: (f64, f64, f64, f64),
}

/// Draws a single key using Cairo.
///
/// This function handles the visual representation of a key, including its shape,
/// border, background color, and text. Text is automatically scaled and truncated
/// to fit within the key's boundaries.
pub fn draw_single_key_cairo(ctx: &Context, key: &KeyDisplay) {
    // Convert key dimensions and properties to f64 for Cairo
    let x = key.center_x as f64;
    let y = key.center_y as f64;
    let width = key.width as f64;
    let height = key.height as f64;
    let corner_radius = key.corner_radius as f64;
    let border_thickness = key.border_thickness as f64;
    let rotation_radians = key.rotation_degrees.to_radians() as f64;

    // Save the current Cairo context state to isolate transformations and style changes.
    ctx.save().expect("Failed to save cairo context state");

    // --- Transformations ---
    // Translate to the key's center, rotate, then translate back by half width/height
    // so that the key is drawn centered at (x,y) and rotated around its center.
    ctx.translate(x, y);
    ctx.rotate(rotation_radians);
    ctx.translate(-width / 2.0, -height / 2.0); // Origin is now top-left of the key box

    // --- Draw Key Shape (Rounded Rectangle) ---
    // Start a new path for the rounded rectangle.
    // The path is constructed by drawing arcs for the corners and lines for the sides.
    // Arcs are drawn in clockwise order starting from the top-right corner.
    ctx.new_sub_path();
    // Top-right corner arc
    ctx.arc(
        width - corner_radius, // center_x of arc
        corner_radius,         // center_y of arc
        corner_radius,         // radius
        -std::f64::consts::PI / 2.0, // start_angle (pointing upwards)
        0.0,                         // end_angle (pointing rightwards)
    );
    // Bottom-right corner arc
    ctx.arc(
        width - corner_radius,
        height - corner_radius,
        corner_radius,
        0.0, // start_angle (pointing rightwards)
        std::f64::consts::PI / 2.0, // end_angle (pointing downwards)
    );
    // Bottom-left corner arc
    ctx.arc(
        corner_radius,
        height - corner_radius,
        corner_radius,
        std::f64::consts::PI / 2.0, // start_angle (pointing downwards)
        std::f64::consts::PI,       // end_angle (pointing leftwards)
    );
    // Top-left corner arc
    ctx.arc(
        corner_radius,
        corner_radius,
        corner_radius,
        std::f64::consts::PI, // start_angle (pointing leftwards)
        3.0 * std::f64::consts::PI / 2.0, // end_angle (pointing upwards)
    );
    ctx.close_path(); // Connects the last arc to the first line/arc, completing the shape.

    // --- Fill Background ---
    let (r, g, b, a) = key.background_color;
    ctx.set_source_rgba(r, g, b, a);
    // Fill the path, but preserve it for stroking the border.
    ctx.fill_preserve().expect("Cairo fill failed");

    // --- Stroke Border ---
    let (r, g, b, a) = key.border_color;
    ctx.set_source_rgba(r, g, b, a);
    ctx.set_line_width(border_thickness);
    ctx.stroke().expect("Cairo stroke failed");

    // --- Draw Text ---
    let (r, g, b, a) = key.text_color;
    ctx.set_source_rgba(r, g, b, a);

    // Use the shared text layout utility
    let text_layout_params = TextLayoutParams {
        text: &key.text,
        key_width_px: width,
        key_height_px: height, // Pass height for more accurate padding calculation
        initial_font_size_pts: key.text_size as f64,
        min_font_size_pts_factor: 0.5, // Example: scale down to 50%
        min_font_size_pts_abs: 6.0,    // Example: absolute minimum 6pt
        padding_factor: 0.1,           // Example: 10% padding
        min_padding_abs: 2.0,          // Example: absolute minimum 2px padding
    };
    let cairo_metrics_provider = CairoMetricsProvider { cairo_ctx: ctx };

    match layout_text(&text_layout_params, &cairo_metrics_provider) {
        Ok(layout_result) => {
            // Font face is set once before calling paint_all_keys (or should be set before this function)
            ctx.set_font_size(layout_result.final_font_size_pts);

            // Recalculate text extents with the final font size for precise centering
            let text_extents = ctx
                .text_extents(&layout_result.final_text)
                .expect("Failed to get text extents for final text");

            // Calculate text position to center it within the key
            // x_bearing is the horizontal displacement from the origin to the leftmost part of the glyphs.
            // y_bearing is the vertical displacement from the origin to the topmost part of the glyphs.
            let text_x = (width - text_extents.width()) / 2.0 - text_extents.x_bearing();
            let text_y = (height - text_extents.height()) / 2.0 - text_extents.y_bearing();

            ctx.move_to(text_x, text_y);
            ctx.show_text(&layout_result.final_text)
                .expect("Cairo show_text failed");
        }
        Err(e) => {
            log::error!("Failed to layout text for key '{}': {}", key.text, e);
            // Optionally, draw a placeholder or error indicator on the key
        }
    }

    // Restore the Cairo context to its state before this function was called.
    ctx.restore()
        .expect("Failed to restore cairo context state");
}

/// Paints all keys onto the Cairo context.
///
/// Clears the context with the background color, then iterates through all
/// `KeyDisplay` items and calls `draw_single_key_cairo` for each.
/// The font face for drawing text on keys must be set on the context before calling this.
pub fn paint_all_keys(
    ctx: &Context,
    keys_to_draw: &Vec<KeyDisplay>,
    background_color: (f64, f64, f64, f64), // Changed from &str to tuple
    font_face: &CairoFontFace, // Font face is now passed in
) {
    // Clear the surface with the provided background color tuple
    let (r, g, b, a) = background_color;
    ctx.save().unwrap();
    ctx.set_source_rgba(r, g, b, a);
    ctx.set_operator(cairo::Operator::Source); // Ensure it overwrites
    ctx.paint().expect("Cairo paint (clear) failed");
    ctx.restore().unwrap();

    if keys_to_draw.is_empty() {
        log::warn!("No keys to draw.");
        return;
    }

    ctx.set_font_face(font_face); // Set font face once

    for key_spec in keys_to_draw {
        // Font size is set per key inside draw_single_key_cairo as it's part of KeyDisplay
        draw_single_key_cairo(ctx, key_spec);
    }
}
