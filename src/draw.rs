// Drawing the keyboard

use cairo::{Context, FontFace as CairoFontFace};

use crate::config::parse_color_string; // Only parse_color_string is needed for background

// Struct to hold key properties for drawing (calculated from KeyConfig and AppState)
// This struct is prepared by AppState::draw and passed to paint_all_keys
#[derive(Debug)]
pub struct KeyDisplay {
    pub text: String,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
    pub border_thickness: f32,
    pub rotation_degrees: f32,
    pub text_size: f32,
    pub border_color: (f64, f64, f64, f64),
    pub background_color: (f64, f64, f64, f64),
    pub text_color: (f64, f64, f64, f64),
}

// New function using Cairo, to be called for each key by the main draw loop/function
// This function can remain pub if there's a reason to draw single keys independently,
// otherwise, it could be private to this module. For now, pub is fine.
pub fn draw_single_key_cairo(ctx: &Context, key: &KeyDisplay) {
    let x = key.center_x as f64;
    let y = key.center_y as f64;
    let width = key.width as f64;
    let height = key.height as f64;
    let corner_radius = key.corner_radius as f64;
    let border_thickness = key.border_thickness as f64;
    let rotation_radians = key.rotation_degrees.to_radians() as f64;

    ctx.save().expect("Failed to save cairo context state");

    ctx.translate(x, y);
    ctx.rotate(rotation_radians);
    ctx.translate(-width / 2.0, -height / 2.0);

    ctx.new_sub_path();
    ctx.arc(
        width - corner_radius,
        corner_radius,
        corner_radius,
        -std::f64::consts::PI / 2.0,
        0.0,
    );
    ctx.arc(
        width - corner_radius,
        height - corner_radius,
        corner_radius,
        0.0,
        std::f64::consts::PI / 2.0,
    );
    ctx.arc(
        corner_radius,
        height - corner_radius,
        corner_radius,
        std::f64::consts::PI / 2.0,
        std::f64::consts::PI,
    );
    ctx.arc(
        corner_radius,
        corner_radius,
        corner_radius,
        std::f64::consts::PI,
        3.0 * std::f64::consts::PI / 2.0,
    );
    ctx.close_path();

    let (r, g, b, a) = key.background_color;
    ctx.set_source_rgba(r, g, b, a);
    ctx.fill_preserve().expect("Cairo fill failed");

    let (r, g, b, a) = key.border_color;
    ctx.set_source_rgba(r, g, b, a);
    ctx.set_line_width(border_thickness);
    ctx.stroke().expect("Cairo stroke failed");

    let (r, g, b, a) = key.text_color;
    ctx.set_source_rgba(r, g, b, a);

    let mut current_text = key.text.clone();
    let mut current_font_size = key.text_size as f64;
    // Font face is set once before calling paint_all_keys
    ctx.set_font_size(current_font_size);

    let text_padding = (key.width * 0.1).min(key.height * 0.1).max(2.0) as f64;
    let max_text_width = width - 2.0 * text_padding;
    let original_font_size = key.text_size as f64;
    let min_font_size = (original_font_size * 0.5).max(6.0);

    let mut text_extents = ctx
        .text_extents(&current_text)
        .expect("Failed to get text extents (initial)");

    while text_extents.width() > max_text_width && current_font_size > min_font_size {
        current_font_size *= 0.9;
        if current_font_size < min_font_size {
            current_font_size = min_font_size;
        }
        ctx.set_font_size(current_font_size);
        text_extents = ctx
            .text_extents(&current_text)
            .expect("Failed to get text extents (scaling)");
        if current_font_size == min_font_size && text_extents.width() > max_text_width {
            break;
        }
    }

    if text_extents.width() > max_text_width {
        let ellipsis = "...";
        let ellipsis_extents = ctx
            .text_extents(ellipsis)
            .expect("Failed to get ellipsis extents");
        let max_width_for_text_with_ellipsis = max_text_width - ellipsis_extents.width();

        while text_extents.width() > max_text_width && !current_text.is_empty() {
            current_text.pop();
            let temp_text_with_ellipsis = if current_text.is_empty() {
                if ellipsis_extents.width() <= max_text_width {
                    ellipsis.to_string()
                } else {
                    "".to_string()
                }
            } else {
                format!("{}{}", current_text, ellipsis)
            };
            text_extents = ctx
                .text_extents(&temp_text_with_ellipsis)
                .expect("Failed to get text extents (truncating)");
            let current_text_only_extents = ctx
                .text_extents(&current_text)
                .expect("Failed to get current_text extents");
            if current_text_only_extents.width() <= max_width_for_text_with_ellipsis
                || current_text.is_empty()
            {
                current_text = temp_text_with_ellipsis;
                text_extents = ctx
                    .text_extents(&current_text)
                    .expect("Failed to get final truncated text extents");
                break;
            }
        }
        if text_extents.width() > max_text_width {
            if ellipsis_extents.width() <= max_text_width {
                current_text = ellipsis.to_string();
            } else if ctx.text_extents("..").unwrap().width() <= max_text_width {
                current_text = "..".to_string();
            } else if ctx.text_extents(".").unwrap().width() <= max_text_width {
                current_text = ".".to_string();
            } else {
                current_text = "".to_string();
            }
        }
    }

    let text_extents = ctx
        .text_extents(&current_text)
        .expect("Failed to get text extents (final)");
    let text_x = (width - text_extents.width()) / 2.0 - text_extents.x_bearing();
    let text_y = (height - text_extents.height()) / 2.0 - text_extents.y_bearing();

    ctx.move_to(text_x, text_y);
    ctx.show_text(&current_text)
        .expect("Cairo show_text failed");

    ctx.restore()
        .expect("Failed to restore cairo context state");
}

pub fn paint_all_keys(
    ctx: &Context,
    keys_to_draw: &Vec<KeyDisplay>,
    bg_color_str: &str,
    font_face: &CairoFontFace, // Font face is now passed in
) {
    // Clear the surface with configured background color
    match parse_color_string(bg_color_str) {
        Ok((r, g, b, a)) => {
            ctx.save().unwrap();
            ctx.set_source_rgba(r, g, b, a);
            ctx.set_operator(cairo::Operator::Source);
            ctx.paint().expect("Cairo paint (clear) failed");
            ctx.restore().unwrap();
        }
        Err(e) => {
            log::error!(
                "Failed to parse background_color_inactive '{}': {}. Using default transparent.",
                bg_color_str,
                e
            );
            ctx.save().unwrap();
            ctx.set_source_rgba(0.0, 0.0, 0.0, 0.0); // Transparent
            ctx.set_operator(cairo::Operator::Source);
            ctx.paint().expect("Cairo paint (clear fallback) failed");
            ctx.restore().unwrap();
        }
    }

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
