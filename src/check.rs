// src/check.rs

use crate::config::{AppConfig, KeyConfig, OverlayConfig, DEFAULT_TEXT_SIZE_UNSCALED};
use crate::text_utils::{layout_text, FreeTypeMetricsProvider, TextLayoutResult, TextLayoutParams};
use freetype::Library as FreeTypeLibrary;
use std::collections::HashMap;

// Helper function for --check: Validate configuration
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

// Helper function for --check: Simulate text scaling and truncation using the shared utility
pub fn simulate_text_layout_for_check(
    key_config: &KeyConfig,
    ft_face: &freetype::Face,
) -> Result<TextLayoutResult, String> {
    let layout_params = TextLayoutParams {
        text: &key_config.name,
        key_width_px: key_config.width as f64,
        key_height_px: key_config.height as f64,
        initial_font_size_pts: key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64,
        min_font_size_pts_factor: 0.5, // Consistent with draw.rs defaults
        min_font_size_pts_abs: 6.0,    // Consistent with draw.rs defaults
        padding_factor: 0.1,           // Consistent with draw.rs defaults
        min_padding_abs: 2.0,          // Consistent with draw.rs defaults
    };

    let ft_metrics_provider = FreeTypeMetricsProvider { ft_face };
    layout_text(&layout_params, &ft_metrics_provider)
}

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

    let font_data: &[u8] = include_bytes!("../default-font/DejaVuSansMono.ttf");
    let ft_library = match FreeTypeLibrary::init() {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("Failed to initialize FreeType library for --check: {:?}", e);
            std::process::exit(1);
        }
    };
    let ft_face = match ft_library.new_memory_face(font_data.to_vec(), 0) {
        Ok(face) => face,
        Err(e) => {
            eprintln!("Failed to load font for --check: {:?}", e);
            std::process::exit(1);
        }
    };

    println!("\nKey Information (Layout from TOML, Text metrics simulated):");
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

            match simulate_text_layout_for_check(key_config_item, &ft_face) { // Updated function call
                Ok(layout_result) => { // Updated variable name
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
                    "{:<20} | {:<25} | {:<10} | {:<10.2} | Error simulating text: {} ",
                    key_config_item.name, bbox_str, key_config_item.keycode, 1.0, e
                );
            }
        }
    }

    print_overlay_config_for_check(&app_config.overlay);

    println!("\nConfiguration check finished.");
    std::process::exit(0);
}
