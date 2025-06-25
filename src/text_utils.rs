// src/text_utils.rs

use freetype::Face as FreeTypeFace;
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
    // pub text_width_px: f64, // Width of the final_text at final_font_size_pts // Removed as per clippy warning (dead_code)
}

/// Common trait for text measurement providers (FreeType and Cairo).
pub trait TextMetricsProvider {
    fn measure_text_width(&self, text: &str, font_size_pts: f64) -> Result<f64, String>;
}

// Implementation for FreeType
pub struct FreeTypeMetricsProvider<'a> {
    pub ft_face: &'a FreeTypeFace,
}

impl<'a> TextMetricsProvider for FreeTypeMetricsProvider<'a> {
    fn measure_text_width(&self, text: &str, font_size_pts: f64) -> Result<f64, String> {
        let pixel_height = font_size_pts.round() as u32;
        if pixel_height == 0 {
            return Ok(0.0);
        }

        self.ft_face
            .set_pixel_sizes(0, pixel_height)
            .map_err(|e| format!("FreeType set_pixel_sizes failed: {:?}", e))?;

        let mut total_width = 0.0;
        for char_code in text.chars() {
            self.ft_face
                .load_char(char_code as usize, freetype::face::LoadFlag::RENDER)
                .map_err(|e| {
                    format!(
                        "FreeType load_char failed for char '{}' (codepoint {}): {:?}",
                        char_code, char_code as u32, e
                    )
                })?;
            total_width += self.ft_face.glyph().advance().x as f64 / 64.0; // 1/64th of a pixel units
        }
        Ok(total_width)
    }
}

// Implementation for Cairo
pub struct CairoMetricsProvider<'a> {
    pub cairo_ctx: &'a CairoContext,
}

impl<'a> TextMetricsProvider for CairoMetricsProvider<'a> {
    fn measure_text_width(&self, text: &str, font_size_pts: f64) -> Result<f64, String> {
        self.cairo_ctx.save().map_err(|e| format!("Cairo save failed: {:?}",e))?;
        self.cairo_ctx.set_font_size(font_size_pts);
        let extents = self.cairo_ctx.text_extents(text).map_err(|e| format!("Cairo text_extents failed: {:?}",e))?;
        self.cairo_ctx.restore().map_err(|e| format!("Cairo restore failed: {:?}",e))?;
        Ok(extents.width())
    }
}


/// Processes text to fit within given constraints by scaling font size and truncating.
pub fn layout_text<TMP: TextMetricsProvider>(
    params: &TextLayoutParams,
    metrics_provider: &TMP,
) -> Result<TextLayoutResult, String> {
    let original_text = params.text.to_string();

    let text_padding = (params.key_width_px * params.padding_factor)
        .min(params.key_height_px * params.padding_factor) // Consider height for padding calc
        .max(params.min_padding_abs);
    let max_text_width_px = (params.key_width_px - 2.0 * text_padding).max(0.0);

    let min_font_size_pts = (params.initial_font_size_pts * params.min_font_size_pts_factor)
        .max(params.min_font_size_pts_abs);

    let mut current_text = original_text.clone();
    let mut current_font_size_pts = params.initial_font_size_pts;

    // --- Font size scaling ---
    let mut text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;

    while text_width_px_at_current_font_size > max_text_width_px && current_font_size_pts > min_font_size_pts {
        current_font_size_pts = (current_font_size_pts * 0.9).max(min_font_size_pts);
        text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;
        if current_font_size_pts == min_font_size_pts && text_width_px_at_current_font_size > max_text_width_px {
             // If even at min font size, it's too wide, break to proceed to truncation.
            break;
        }
    }

    // --- Text truncation ---
    let mut truncated_chars = 0;
    // Re-check width at the potentially reduced font size before starting truncation
    let mut text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;

    if text_width_px_at_current_font_size > max_text_width_px {
        let ellipsis = "...";
        let ellipsis_width_px = metrics_provider.measure_text_width(ellipsis, current_font_size_pts)?;

        // Try to fit text with ellipsis
        while text_width_px_at_current_font_size > max_text_width_px && !current_text.is_empty() {
            current_text.pop(); // Remove last character
            truncated_chars = original_text.chars().count() - current_text.chars().count();

            if current_text.is_empty() { // All original text removed
                current_text = if ellipsis_width_px <= max_text_width_px {
                    ellipsis.to_string()
                } else {
                    // If ellipsis itself doesn't fit, try to shorten ellipsis
                    let mut short_ellipsis = ellipsis.to_string();
                    while metrics_provider.measure_text_width(&short_ellipsis, current_font_size_pts)? > max_text_width_px && !short_ellipsis.is_empty() {
                        short_ellipsis.pop();
                    }
                    short_ellipsis
                };
                // text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?; // No longer needed to assign here
                break;
            }

            let temp_text_with_ellipsis = format!("{}{}", current_text, ellipsis);
            text_width_px_at_current_font_size = metrics_provider.measure_text_width(&temp_text_with_ellipsis, current_font_size_pts)?;

            // Check if current_text + ellipsis fits
            let current_text_only_width_px = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;
            if current_text_only_width_px + ellipsis_width_px <= max_text_width_px {
                current_text = temp_text_with_ellipsis;
                // text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?; // No longer needed to assign here
                break;
            }
        }

        // If after all truncation, it's still too wide (e.g. very narrow key, ellipsis doesn't fit)
        // this could happen if ellipsis_width_px > max_text_width_px initially.
        // Re-check width after potential truncation.
        text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;
        if text_width_px_at_current_font_size > max_text_width_px {
             let mut short_ellipsis = ellipsis.to_string();
             while metrics_provider.measure_text_width(&short_ellipsis, current_font_size_pts)? > max_text_width_px && !short_ellipsis.is_empty() {
                 short_ellipsis.pop();
             }
             current_text = short_ellipsis;
             // text_width_px_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?; // No longer needed to assign here
             // Update truncated_chars if original text is completely replaced by a (possibly shortened) ellipsis
             if current_text.len() <= ellipsis.len() && !original_text.starts_with(&current_text) {
                truncated_chars = original_text.chars().count();
             }
        }
    }

    Ok(TextLayoutResult {
        final_text: current_text,
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
        // text_width_px, // Removed as per clippy warning (dead_code)
    })
}
