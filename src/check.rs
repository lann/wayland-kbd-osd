// src/check.rs

//! This module implements the `--check` functionality for the application.
//! It validates the configuration file, simulates text layout for keys,
//! and prints diagnostic information about the parsed configuration.

use crate::config::{AppConfig, KeyConfig, OverlayConfig, DEFAULT_TEXT_SIZE_UNSCALED};
use crate::text_utils::{layout_text, TextLayoutResult, TextLayoutParams};
use cairo::{Context as CairoContext, FontFace as CairoFontFace, ImageSurface, Format};
use std::collections::HashMap;

/// Validates the application configuration for common issues.
///
/// Checks for:
/// - Overlapping keys (basic bounding box check, ignoring rotation).
/// - Duplicate keycodes.
/// - Invalid values like non-positive width/height for keys, or negative text/border/radius values.
///
/// # Arguments
///
/// * `config` - A reference to the `AppConfig` to validate.
///
/// # Returns
///
/// * `Ok(())` if the configuration passes all checks.
/// * `Err(String)` with a descriptive error message if validation fails.
pub fn validate_config(config: &AppConfig) -> Result<(), String> {
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

            if k1_left < k2_right && k1_right > k2_left && k1_top < k2_bottom && k1_bottom > k2_top
            {
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
            return Err(format!(
                "Configuration validation error: Key '{}' has non-positive width {:.1}.",
                key_config.name, key_config.width
            ));
        }
        if key_config.height <= 0.0 {
            return Err(format!(
                "Configuration validation error: Key '{}' has non-positive height {:.1}.",
                key_config.name, key_config.height
            ));
        }
        if let Some(ts) = key_config.text_size {
            if ts <= 0.0 {
                return Err(format!(
                    "Configuration validation error: Key '{}' has non-positive text_size {:.1}.",
                    key_config.name, ts
                ));
            }
        }
        if let Some(cr) = key_config.corner_radius {
            if cr < 0.0 {
                return Err(format!(
                    "Configuration validation error: Key '{}' has negative corner_radius {:.1}.",
                    key_config.name, cr
                ));
            }
        }
        if let Some(bt) = key_config.border_thickness {
            if bt < 0.0 {
                return Err(format!(
                    "Configuration validation error: Key '{}' has negative border_thickness {:.1}.",
                    key_config.name, bt
                ));
            }
        }
    }
    Ok(())
}

/// Prints a summary of the `OverlayConfig` to standard output.
///
/// This function is used by the `--check` command to display overlay-related
/// configuration values in a human-readable format, including parsed color values.
///
/// # Arguments
///
/// * `config` - A reference to the `OverlayConfig` to print.
pub fn print_overlay_config_for_check(config: &OverlayConfig) {
    println!("\nOverlay Configuration:");
    println!(
        "  Screen:               {}",
        config.screen.as_deref().unwrap_or("Compositor default")
    );
    println!("  Position:             {:?}", config.position);

    let width_str = match config.size_width {
        Some(crate::config::SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(crate::config::SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from height/layout".to_string(),
    };
    let height_str = match config.size_height {
        Some(crate::config::SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(crate::config::SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from width/layout".to_string(),
    };
    println!("  Size Width:           {}", width_str);
    println!("  Size Height:          {}", height_str);

    println!(
        "  Margins (T,R,B,L):    {}, {}, {}, {}",
        config.margin_top, config.margin_right, config.margin_bottom, config.margin_left
    );

    match crate::config::parse_color_string(&config.background_color_inactive) {
        Ok((r, g, b, a)) => println!(
            "  Background Inactive:  {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2})",
            config.background_color_inactive, r, g, b, a
        ),
        Err(e) => println!(
            "  Background Inactive:  {} (Error: {})",
            config.background_color_inactive, e
        ),
    }
    match crate::config::parse_color_string(&config.background_color_active) {
        Ok((r,g,b,a)) => println!("  Background Active:    {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2}) (currently unused for global bg)", config.background_color_active, r,g,b,a),
        Err(e) => println!("  Background Active:    {} (Error: {})", config.background_color_active, e),
    }
    match crate::config::parse_color_string(&config.default_key_background_color) {
        Ok((r, g, b, a)) => println!(
            "  Default Key BG:       {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2})",
            config.default_key_background_color, r, g, b, a
        ),
        Err(e) => println!(
            "  Default Key BG:       {} (Error: {})",
            config.default_key_background_color, e
        ),
    }
}

/// Simulates text layout for a given key using a Cairo context.
///
/// This function utilizes `crate::text_utils::layout_text` to determine how
/// a key's label would be scaled and truncated to fit its defined dimensions.
/// It's used by the `--check` command to provide feedback on text rendering.
///
/// # Arguments
///
/// * `key_config` - A reference to the `KeyConfig` for the key.
/// * `cairo_ctx` - A reference to a Cairo `Context` initialized with the
///   appropriate font face.
///
/// # Returns
///
/// * `Ok(TextLayoutResult)` containing the final text, font size, and truncation info.
/// * `Err(String)` if text layout simulation fails.
pub fn simulate_text_layout_for_check(
    key_config: &KeyConfig,
    cairo_ctx: &CairoContext,
) -> Result<TextLayoutResult, String> {
    let layout_params = TextLayoutParams {
        text: &key_config.name,
        key_width_px: key_config.width as f64,
        key_height_px: key_config.height as f64,
        initial_font_size_pts: key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64,
        min_font_size_pts_factor: 0.5,
        min_font_size_pts_abs: 6.0,
        padding_factor: 0.1,
        min_padding_abs: 2.0,
    };

    // Pass the context directly to layout_text
    layout_text(&layout_params, cairo_ctx)
}

/// Runs the configuration check process.
///
/// This is the main entry point for the `--check` command. It performs:
/// 1. Basic configuration validation (`validate_config`).
/// 2. Sets up a dummy Cairo context with the default font.
/// 3. Iterates through each key, simulating text layout (`simulate_text_layout_for_check`)
///    and printing information about its dimensions, keycode, and how its label fits.
/// 4. Prints the overlay configuration details (`print_overlay_config_for_check`).
///
/// Exits with status code 0 on success, or 1 if errors are found or setup fails.
///
/// # Arguments
///
/// * `config_path` - The path to the configuration file being checked (for display purposes).
/// * `app_config` - A reference to the loaded `AppConfig`.
pub fn run_check(config_path: &str, app_config: &AppConfig) {
    println!(
        "Performing configuration check for '{}'...",
        config_path
    );

    if let Err(e) = validate_config(app_config) {
        eprintln!("Configuration validation failed: {}", e);
        std::process::exit(1);
    } else {
        println!("Basic validation (overlaps, duplicates, positive dimensions) passed.");
    }

    let surface = ImageSurface::create(Format::A1, 1, 1)
        .map_err(|e| format!("Failed to create Cairo ImageSurface for --check: {:?}", e));
    if surface.is_err() {
        eprintln!("{}", surface.err().unwrap());
        std::process::exit(1);
    }
    let surface = surface.unwrap();

    let cairo_ctx = CairoContext::new(&surface)
        .map_err(|e| format!("Failed to create Cairo Context for --check: {:?}", e));
    if cairo_ctx.is_err() {
        eprintln!("{}", cairo_ctx.err().unwrap());
        std::process::exit(1);
    }
    let cairo_ctx = cairo_ctx.unwrap();

    let font_data: &[u8] = include_bytes!("../default-font/DejaVuSansMono.ttf");
    // Create a FontFace from the embedded font data using FreeType, then bridge to Cairo.
    // This is necessary because cairo-rs (0.19) doesn't provide a direct "font_face_create_from_data"
    // without an existing FreeType face object when using its FreeType backend.
    // The `freetype-rs` crate is used here as a loader for the `cairo::FontFace`.
    let ft_library = freetype::Library::init().expect("FT init for cairo font face in check");
    let ft_face = ft_library.new_memory_face(font_data.to_vec(), 0).expect("FT face for cairo font face in check");
    let cairo_font_face = CairoFontFace::create_from_ft(&ft_face).expect("Cairo FT face creation for check");

    cairo_ctx.set_font_face(&cairo_font_face);

    println!("\nKey Information (Layout from TOML, Text metrics simulated with Cairo):");
    println!(
        "{:<20} | {:<25} | {:<10} | {:<10} | {:<20}",
        "Label (Name)", "Bounding Box (L,T,R,B)", "Keycode", "Font Scale", "Truncated Label"
    );
    println!(
        "{:-<20}-+-{:-<25}-+-{:-<10}-+-{:-<10}-+-{:-<20}",
        "", "", "", "", ""
    );

    for key_config_item in &app_config.key {
        let right_edge = key_config_item.left + key_config_item.width;
        let bottom_edge = key_config_item.top + key_config_item.height;
        let bbox_str = format!(
            "{:.1},{:.1}, {:.1},{:.1}",
            key_config_item.left, key_config_item.top, right_edge, bottom_edge
        );

        let initial_font_size = key_config_item
            .text_size
            .unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64;

            match simulate_text_layout_for_check(key_config_item, &cairo_ctx) {
                Ok(layout_result) => {
                let font_scale = if initial_font_size > 0.0 {
                        layout_result.final_font_size_pts / initial_font_size
                } else {
                    1.0
                };

                    let truncated_label_display = if layout_result.truncated_chars > 0
                        || !layout_result.final_text.eq(&key_config_item.name)
                {
                        layout_result.final_text
                } else {
                    "".to_string()
                };

                println!(
                    "{:<20} | {:<25} | {:<10} | {:<10.2} | {:<20}",
                    key_config_item.name,
                    bbox_str,
                    key_config_item.keycode,
                    font_scale,
                    truncated_label_display
                );
            }
            Err(e) => {
                println!(
                    "{:<20} | {:<25} | {:<10} | {:<10.2} | Error simulating text with Cairo: {} ",
                    key_config_item.name, bbox_str, key_config_item.keycode, 1.0, e
                );
            }
        }
    }

    print_overlay_config_for_check(&app_config.overlay);

    println!("\nConfiguration check finished.");
    std::process::exit(0);
}
