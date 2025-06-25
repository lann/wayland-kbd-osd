// src/text_utils.rs

//! This module provides utilities for laying out text within constrained boundaries,
//! primarily for rendering key labels. It handles font size scaling and text truncation.

use cairo::Context as CairoContext;

/// Parameters defining how text should be laid out within a key.
///
/// These parameters control the initial font size, scaling limits, padding,
/// and the text content itself.
pub struct TextLayoutParams<'a> {
    /// The text string to lay out.
    pub text: &'a str,
    /// The available width in pixels for the text, including padding.
    pub key_width_px: f64,
    /// The available height in pixels for the text, used for padding calculation.
    pub key_height_px: f64,
    /// The initial desired font size in points.
    pub initial_font_size_pts: f64,
    /// Factor by which the initial font size can be reduced (e.g., 0.5 means 50%).
    pub min_font_size_pts_factor: f64,
    /// An absolute minimum font size in points, overriding the factor if larger.
    pub min_font_size_pts_abs: f64,
    /// Padding factor relative to the smaller of key_width_px or key_height_px.
    pub padding_factor: f64,
    /// Absolute minimum padding in pixels, overriding the factor if larger.
    pub min_padding_abs: f64,
}

/// Represents the result of a text layout operation.
///
/// Contains the final text (possibly truncated), the determined font size,
/// and the number of characters that were truncated.
#[derive(Debug, Clone)]
pub struct TextLayoutResult {
    /// The final text string to be rendered. This may be truncated if the original
    /// text did not fit.
    pub final_text: String,
    /// The final font size in points determined by the layout process.
    pub final_font_size_pts: f64,
    /// The number of characters truncated from the original text.
    /// If 0, the text was not truncated (though it might have been scaled).
    pub truncated_chars: usize,
}

/// Measures the width of a given text string using the provided Cairo context and font size.
///
/// This is a helper function that encapsulates saving/restoring the Cairo context
/// and setting the font size before measuring text extents.
///
/// # Arguments
/// * `ctx` - The Cairo `Context` to use for measurement.
/// * `text` - The text string to measure.
/// * `font_size_pts` - The font size in points to use for measurement.
///
/// # Returns
/// * `Ok(f64)` with the measured text width in pixels.
/// * `Err(String)` if any Cairo operation fails.
fn measure_text_width_with_cairo(ctx: &CairoContext, text: &str, font_size_pts: f64) -> Result<f64, String> {
    ctx.save().map_err(|e| format!("Cairo save failed: {:?}", e))?;
    ctx.set_font_size(font_size_pts);
    let extents = ctx.text_extents(text).map_err(|e| format!("Cairo text_extents failed: {:?}", e))?;
    ctx.restore().map_err(|e| format!("Cairo restore failed: {:?}", e))?;
    Ok(extents.width())
}

/// Processes text to fit within given constraints by scaling font size and then truncating.
///
/// The function first attempts to fit the text by reducing the font size down to a
/// calculated minimum. If the text still doesn't fit, it will be truncated from the end,
/// character by character, until it fits. No ellipsis is added.
///
/// Text measurement is performed using the provided Cairo context, which must have
/// the desired font face already set.
///
/// # Arguments
/// * `params` - A `TextLayoutParams` struct defining the text, constraints, and layout rules.
/// * `ctx` - The Cairo `Context` used for text measurement.
///
/// # Returns
/// * `Ok(TextLayoutResult)` containing the final text, font size, and truncation info.
/// * `Err(String)` if any internal Cairo operation fails during measurement.
pub fn layout_text(
    params: &TextLayoutParams,
    ctx: &CairoContext,
) -> Result<TextLayoutResult, String> {
    let original_text_char_count = params.text.chars().count();

    // Calculate effective padding and maximum allowable width for the text.
    let text_padding = (params.key_width_px * params.padding_factor)
        .min(params.key_height_px * params.padding_factor) // Use min of width/height factor for symmetrical feel
        .max(params.min_padding_abs);
    let max_text_width_px = (params.key_width_px - 2.0 * text_padding).max(0.0);

    // Determine the minimum allowable font size.
    let min_font_size_pts = (params.initial_font_size_pts * params.min_font_size_pts_factor)
        .max(params.min_font_size_pts_abs);

    let mut current_text = params.text.to_string();
    let mut current_font_size_pts = params.initial_font_size_pts;

    // --- Step 1: Font size scaling ---
    // Try to fit the text by reducing font size, down to the minimum allowed.
    let mut text_width_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;

    while text_width_at_current_font_size > max_text_width_px && current_font_size_pts > min_font_size_pts {
        // Reduce font size by a small factor, but not below the absolute minimum.
        current_font_size_pts = (current_font_size_pts * 0.9).max(min_font_size_pts);
        text_width_at_current_font_size = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;

        // If we've hit the minimum font size and it still doesn't fit, break to truncation.
        if current_font_size_pts == min_font_size_pts && text_width_at_current_font_size > max_text_width_px {
            break;
        }
    }

    // --- Step 2: Text truncation ---
    // If text still doesn't fit after font scaling, truncate it from the end.
    // No ellipsis is added.
    // We need to re-measure with the potentially scaled font size.
    let mut text_width_after_scaling = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;

    if text_width_after_scaling > max_text_width_px {
        // Iterate by grapheme clusters to handle Unicode correctly if we were using a more complex
        // string type, but String::pop() works on char boundaries which is acceptable here.
        // For perfect grapheme cluster handling, a crate like `unicode-segmentation` would be needed
        // if we were to split the string differently, but pop() is fine for simple truncation.
        while text_width_after_scaling > max_text_width_px && !current_text.is_empty() {
            current_text.pop(); // Remove the last character
            if current_text.is_empty() {
                text_width_after_scaling = 0.0; // Empty string has zero width
                break;
            }
            text_width_after_scaling = measure_text_width_with_cairo(ctx, &current_text, current_font_size_pts)?;
        }
    }

    let final_text_char_count = current_text.chars().count();
    let truncated_chars = original_text_char_count - final_text_char_count;

    Ok(TextLayoutResult {
        final_text: current_text,
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
    })
}
