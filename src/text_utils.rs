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
    // pub text_width_px: f64, // Width of the final_text at final_font_size_pts - Clippy: dead_code
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
    // Initial width calculation
    let mut text_width_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;

    while text_width_at_current_font_size > max_text_width_px && current_font_size_pts > min_font_size_pts {
        current_font_size_pts = (current_font_size_pts * 0.9).max(min_font_size_pts);
        text_width_at_current_font_size = metrics_provider.measure_text_width(&current_text, current_font_size_pts)?;

        if current_font_size_pts == min_font_size_pts && text_width_at_current_font_size > max_text_width_px {
            break;
        }
    }

    // --- Text truncation ---
    let mut truncated_chars = 0;
    // Use the text_width_at_current_font_size which reflects the width after potential font scaling
    if text_width_at_current_font_size > max_text_width_px {
        let ellipsis = "...";
        // ellipsis_width_px was removed as it was unused. Calculations are done with "text..." directly.

        let mut current_text_for_truncation = current_text.clone(); // Work with a copy for truncation

        loop {
            let width_of_text_for_truncation = metrics_provider.measure_text_width(&current_text_for_truncation, current_font_size_pts)?;
            if width_of_text_for_truncation <= max_text_width_px {
                current_text = current_text_for_truncation; // No ellipsis needed or already fits
                break;
            }

            if current_text_for_truncation.is_empty() { // Cannot truncate further
                 // Try to fit ellipsis, then shorter versions
                current_text = ellipsis.to_string();
                while metrics_provider.measure_text_width(&current_text, current_font_size_pts)? > max_text_width_px && current_text.len() > 1 {
                    current_text.pop();
                }
                if metrics_provider.measure_text_width(&current_text, current_font_size_pts)? > max_text_width_px && !current_text.is_empty() {
                    current_text.clear(); // Ellipsis (even ".") doesn't fit
                }
                truncated_chars = original_text.chars().count();
                break;
            }

            // Try with ellipsis
            let temp_text_with_ellipsis = format!("{}{}", current_text_for_truncation, ellipsis);
            if metrics_provider.measure_text_width(&temp_text_with_ellipsis, current_font_size_pts)? <= max_text_width_px {
                current_text = temp_text_with_ellipsis;
                truncated_chars = original_text.chars().count() - current_text_for_truncation.chars().count();
                break;
            }

            // Pop character if ellipsis version is still too long
            current_text_for_truncation.pop();
            if current_text_for_truncation.is_empty() { // Check again after pop for loop termination with ellipsis
                 current_text = ellipsis.to_string();
                 while metrics_provider.measure_text_width(&current_text, current_font_size_pts)? > max_text_width_px && current_text.len() > 1 {
                    current_text.pop();
                 }
                 if metrics_provider.measure_text_width(&current_text, current_font_size_pts)? > max_text_width_px && !current_text.is_empty() {
                    current_text.clear();
                 }
                 truncated_chars = original_text.chars().count();
                 break;
            }
        }
    }

    Ok(TextLayoutResult {
        final_text: current_text,
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
        // text_width_px, // Field commented out due to Clippy dead_code lint
    })
}
