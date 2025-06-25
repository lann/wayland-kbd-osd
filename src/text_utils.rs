// src/text_utils.rs

use cairo::Context as CairoContext;

/// Configuration for text layout.
pub struct TextLayoutParams<'a> {
    pub text: &'a str,
    pub key_width_px: f64,
    pub key_height_px: f64, // Currently unused by common logic, but good for context
    pub initial_font_size_pts: f64,
    pub min_font_size_pts_factor: f64, // e.g., 0.5 for 50% of initial
    pub min_font_size_pts_abs: f64,    // e.g., 6.0 pts
    pub padding_factor: f64,           // e.g., 0.1 for 10% of min(width, height)
    pub min_padding_abs: f64,          // e.g., 2.0 px
}

/// Result of text layout processing.
#[derive(Debug, Clone)]
pub struct TextLayoutResult {
    pub final_text: String,
    pub final_font_size_pts: f64,
    pub truncated_chars: usize,
}

// Helper function to measure text width using Cairo context
// This function encapsulates the logic previously in CairoMetricsProvider
fn measure_text_width_with_cairo(ctx: &CairoContext, text: &str, font_size_pts: f64) -> Result<f64, String> {
    ctx.save().map_err(|e| format!("Cairo save failed: {:?}", e))?;
    ctx.set_font_size(font_size_pts);
    let extents = ctx.text_extents(text).map_err(|e| format!("Cairo text_extents failed: {:?}", e))?;
    ctx.restore().map_err(|e| format!("Cairo restore failed: {:?}", e))?;
    Ok(extents.width())
}

/// Processes text to fit within given constraints by scaling font size and truncating.
/// Text measurement is performed using the provided Cairo context.
pub fn layout_text(
    params: &TextLayoutParams,
    ctx: &CairoContext, // Takes CairoContext directly
) -> Result<TextLayoutResult, String> {
    let original_text = params.text.to_string();

    let text_padding = (params.key_width_px * params.padding_factor)
        .min(params.key_height_px * params.padding_factor)
        .max(params.min_padding_abs);
    let max_text_width_px = (params.key_width_px - 2.0 * text_padding).max(0.0);

    let min_font_size_pts = (params.initial_font_size_pts * params.min_font_size_pts_factor)
        .max(params.min_font_size_pts_abs);

    let mut current_text = original_text.clone();
    let mut current_font_size_pts = params.initial_font_size_pts;

    // --- Font size scaling ---
    let mut text_width_px_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;

    while text_width_px_at_current_font_size > max_text_width_px && current_font_size_pts > min_font_size_pts {
        current_font_size_pts = (current_font_size_pts * 0.9).max(min_font_size_pts);
        text_width_px_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;
        if current_font_size_pts == min_font_size_pts && text_width_px_at_current_font_size > max_text_width_px {
            break;
        }
    }

    // --- Text truncation ---
    let mut truncated_chars = 0;
    let mut text_width_px_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;

    if text_width_px_at_current_font_size > max_text_width_px {
        let ellipsis = "...";
        let ellipsis_width_px = measure_text_width_with_cairo(ctx, ellipsis, current_font_size_pts)?;

        while text_width_px_at_current_font_size > max_text_width_px && !current_text.is_empty() {
            current_text.pop();
            truncated_chars = original_text.chars().count() - current_text.chars().count();

            if current_text.is_empty() {
                current_text = if ellipsis_width_px <= max_text_width_px {
                    ellipsis.to_string()
                } else {
                    let mut short_ellipsis = ellipsis.to_string();
                    while measure_text_width_with_cairo(ctx, &short_ellipsis, current_font_size_pts)? > max_text_width_px && !short_ellipsis.is_empty() {
                        short_ellipsis.pop();
                    }
                    short_ellipsis
                };
                break;
            }

            let temp_text_with_ellipsis = format!("{}{}", current_text, ellipsis);
            text_width_px_at_current_font_size = measure_text_width_with_cairo(ctx, &temp_text_with_ellipsis, current_font_size_pts)?;

            let current_text_only_width_px = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;
            if current_text_only_width_px + ellipsis_width_px <= max_text_width_px {
                current_text = temp_text_with_ellipsis;
                break;
            }
        }

        text_width_px_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;
        if text_width_px_at_current_font_size > max_text_width_px {
             let mut short_ellipsis = ellipsis.to_string();
             while measure_text_width_with_cairo(ctx, &short_ellipsis, current_font_size_pts)? > max_text_width_px && !short_ellipsis.is_empty() {
                 short_ellipsis.pop();
             }
             current_text = short_ellipsis;
             if current_text.len() <= ellipsis.len() && !original_text.starts_with(&current_text) {
                truncated_chars = original_text.chars().count();
             }
        }
    }

    Ok(TextLayoutResult {
        final_text: current_text,
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
    })
}
