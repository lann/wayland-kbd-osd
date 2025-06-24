// Configuration loading and related structs
use serde::Deserialize;
use serde_value::Value as SerdeValue;
use std::collections::HashMap;
use std::fs;
// use std::process; // This was unused

use crate::keycodes;

// Represents a size that can be absolute (pixels) or relative (ratio of screen)
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(untagged)] // Allows parsing "100" as pixels(100) or "0.5" as ratio(0.5)
pub enum SizeDimension {
    Pixels(u32),
    Ratio(f32),
}

// Enum for specifying overlay position
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    CenterLeft,
    CenterRight,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OverlayConfig {
    #[serde(default)]
    pub screen: Option<String>,
    #[serde(default = "default_overlay_position")]
    pub position: OverlayPosition,
    pub size_width: Option<SizeDimension>,
    pub size_height: Option<SizeDimension>,
    #[serde(default = "default_overlay_margin")]
    pub margin_top: i32,
    #[serde(default = "default_overlay_margin")]
    pub margin_right: i32,
    #[serde(default = "default_overlay_margin")]
    pub margin_bottom: i32,
    #[serde(default = "default_overlay_margin")]
    pub margin_left: i32,
    #[serde(default = "default_background_color_inactive")]
    pub background_color_inactive: String,
    #[serde(default = "default_background_color_active")]
    pub background_color_active: String,
    #[serde(default = "default_key_background_color_string")]
    pub default_key_background_color: String,
    #[serde(default = "default_key_text_color_string")]
    pub default_key_text_color: String,
    #[serde(default = "default_key_outline_color_string")]
    pub default_key_outline_color: String,
    #[serde(default = "default_active_key_background_color_string")]
    pub active_key_background_color: String,
    #[serde(default = "default_active_key_text_color_string")]
    pub active_key_text_color: String,
}

fn default_overlay_position() -> OverlayPosition {
    OverlayPosition::BottomCenter
}
fn default_overlay_margin() -> i32 {
    0
}
fn default_background_color_inactive() -> String {
    "#00000000".to_string()
}
fn default_background_color_active() -> String {
    "#A0A0A0D0".to_string()
}
pub fn default_key_background_color_string() -> String {
    "#4D4D4D80".to_string()
}
fn default_key_text_color_string() -> String {
    "#B3B3B3CC".to_string()
}
fn default_key_outline_color_string() -> String {
    "#B3B3B3CC".to_string()
}
fn default_active_key_background_color_string() -> String {
    "#A0A0F0FF".to_string()
}
fn default_active_key_text_color_string() -> String {
    default_key_text_color_string()
}

impl Default for OverlayConfig {
    fn default() -> Self {
        OverlayConfig {
            screen: None,
            position: default_overlay_position(),
            size_width: None,
            size_height: Some(SizeDimension::Ratio(0.3)),
            margin_top: default_overlay_margin(),
            margin_right: default_overlay_margin(),
            margin_bottom: default_overlay_margin(),
            margin_left: default_overlay_margin(),
            background_color_inactive: default_background_color_inactive(),
            background_color_active: default_background_color_active(),
            default_key_background_color: default_key_background_color_string(),
            default_key_text_color: default_key_text_color_string(),
            default_key_outline_color: default_key_outline_color_string(),
            active_key_background_color: default_active_key_background_color_string(),
            active_key_text_color: default_active_key_text_color_string(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct KeyConfig {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    #[serde(alias = "keycode")]
    pub raw_keycode: Option<SerdeValue>,
    #[serde(skip_deserializing)]
    pub keycode: u32,
    pub rotation_degrees: Option<f32>,
    pub text_size: Option<f32>,
    pub corner_radius: Option<f32>,
    pub border_thickness: Option<f32>,
    pub background_color: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub key: Vec<KeyConfig>,
    #[serde(default)]
    pub overlay: OverlayConfig,
}

// Helper function to parse color string like "#RRGGBBAA" or "#RGB"
// Returns (r, g, b, a) tuple with values from 0.0 to 1.0
pub fn parse_color_string(color_str: &str) -> Result<(f64, f64, f64, f64), String> {
    let s = color_str.trim_start_matches('#');
    match s.len() {
        6 => {
            // RRGGBB
            let r = u8::from_str_radix(&s[0..2], 16)
                .map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&s[2..4], 16)
                .map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&s[4..6], 16)
                .map_err(|e| format!("Invalid hex for B: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0)) // Default alpha to 1.0
        }
        8 => {
            // RRGGBBAA
            let r = u8::from_str_radix(&s[0..2], 16)
                .map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&s[2..4], 16)
                .map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&s[4..6], 16)
                .map_err(|e| format!("Invalid hex for B: {}", e))?;
            let a = u8::from_str_radix(&s[6..8], 16)
                .map_err(|e| format!("Invalid hex for A: {}", e))?;
            Ok((
                r as f64 / 255.0,
                g as f64 / 255.0,
                b as f64 / 255.0,
                a as f64 / 255.0,
            ))
        }
        3 => {
            // RGB
            let r_char = s.chars().next().unwrap();
            let g_char = s.chars().nth(1).unwrap();
            let b_char = s.chars().nth(2).unwrap();
            let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16)
                .map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16)
                .map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16)
                .map_err(|e| format!("Invalid hex for B: {}", e))?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0))
        }
        4 => {
            // RGBA
            let r_char = s.chars().next().unwrap();
            let g_char = s.chars().nth(1).unwrap();
            let b_char = s.chars().nth(2).unwrap();
            let a_char = s.chars().nth(3).unwrap();
            let r = u8::from_str_radix(&format!("{}{}", r_char, r_char), 16)
                .map_err(|e| format!("Invalid hex for R: {}", e))?;
            let g = u8::from_str_radix(&format!("{}{}", g_char, g_char), 16)
                .map_err(|e| format!("Invalid hex for G: {}", e))?;
            let b = u8::from_str_radix(&format!("{}{}", b_char, b_char), 16)
                .map_err(|e| format!("Invalid hex for B: {}", e))?;
            let a = u8::from_str_radix(&format!("{}{}", a_char, a_char), 16)
                .map_err(|e| format!("Invalid hex for A: {}", e))?;
            Ok((
                r as f64 / 255.0,
                g as f64 / 255.0,
                b as f64 / 255.0,
                a as f64 / 255.0,
            ))
        }
        _ => Err(format!(
            "Invalid color string length for '{}'. Expected #RRGGBB, #RRGGBBAA, #RGB, or #RGBA",
            color_str
        )),
    }
}

// Default appearance values (unscaled) - also used in check_config and AppState::draw
pub const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
pub const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
pub const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;
pub const DEFAULT_ROTATION_DEGREES: f32 = 0.0;

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
        Some(SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from height/layout".to_string(),
    };
    let height_str = match config.size_height {
        Some(SizeDimension::Pixels(px)) => format!("{}px", px),
        Some(SizeDimension::Ratio(r)) => format!("{:.0}% screen", r * 100.0),
        None => "Derived from width/layout".to_string(),
    };
    println!("  Size Width:           {}", width_str);
    println!("  Size Height:          {}", height_str);

    println!(
        "  Margins (T,R,B,L):    {}, {}, {}, {}",
        config.margin_top, config.margin_right, config.margin_bottom, config.margin_left
    );

    match parse_color_string(&config.background_color_inactive) {
        Ok((r, g, b, a)) => println!(
            "  Background Inactive:  {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2})",
            config.background_color_inactive, r, g, b, a
        ),
        Err(e) => println!(
            "  Background Inactive:  {} (Error: {})",
            config.background_color_inactive, e
        ),
    }
    match parse_color_string(&config.background_color_active) {
        Ok((r,g,b,a)) => println!("  Background Active:    {} (R:{:.2} G:{:.2} B:{:.2} A:{:.2}) (currently unused for global bg)", config.background_color_active, r,g,b,a),
        Err(e) => println!("  Background Active:    {} (Error: {})", config.background_color_active, e),
    }
    match parse_color_string(&config.default_key_background_color) {
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

// Helper struct for --check: Text metrics simulation result
pub struct TextCheckResult {
    pub final_font_size_pts: f64,
    pub truncated_chars: usize,
    pub final_text: String,
}

// Helper function for --check: Simulate text scaling and truncation
pub fn simulate_text_layout(
    key_config: &KeyConfig,
    ft_face: &freetype::Face,
) -> Result<TextCheckResult, String> {
    let original_text = key_config.name.clone();
    let key_width = key_config.width as f64;
    let key_height = key_config.height as f64;

    let original_font_size_pts = key_config.text_size.unwrap_or(DEFAULT_TEXT_SIZE_UNSCALED) as f64;

    let text_padding = (key_width * 0.1).min(key_height * 0.1).max(2.0);
    let max_text_width_px = key_width - 2.0 * text_padding;

    let min_font_size_pts = (original_font_size_pts * 0.5).max(6.0);

    let mut current_text = original_text.clone();
    let mut current_font_size_pts = original_font_size_pts;
    let mut truncated_chars = 0;

    let get_ft_text_width = |text: &str,
                             size_pts: f64,
                             face: &freetype::Face|
     -> Result<f64, String> {
        let pixel_height = size_pts.round() as u32;
        if pixel_height == 0 {
            return Ok(0.0);
        }

        face.set_pixel_sizes(0, pixel_height)
            .map_err(|e| format!("FreeType set_pixel_sizes failed: {:?}", e))?;

        let mut total_width = 0.0;
        for char_code in text.chars() {
            face.load_char(char_code as usize, freetype::face::LoadFlag::RENDER)
                .map_err(|e| format!("FreeType load_char failed for '{}': {:?}", char_code, e))?;
            total_width += face.glyph().advance().x as f64 / 64.0;
        }
        Ok(total_width)
    };

    let mut text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;

    while text_width_px > max_text_width_px && current_font_size_pts > min_font_size_pts {
        current_font_size_pts *= 0.9;
        if current_font_size_pts < min_font_size_pts {
            current_font_size_pts = min_font_size_pts;
        }
        text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
        if current_font_size_pts == min_font_size_pts && text_width_px > max_text_width_px {
            break;
        }
    }

    if text_width_px > max_text_width_px {
        let ellipsis = "...";
        let ellipsis_width_px = get_ft_text_width(ellipsis, current_font_size_pts, ft_face)?;

        while text_width_px > max_text_width_px && !current_text.is_empty() {
            // let initial_len_before_pop = current_text.chars().count(); // Unused
            current_text.pop();
            truncated_chars = original_text.chars().count() - current_text.chars().count();

            if current_text.is_empty() {
                current_text = if ellipsis_width_px <= max_text_width_px {
                    ellipsis.to_string()
                } else {
                    "".to_string()
                };
                text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
                break;
            }

            let temp_text_with_ellipsis = format!("{}{}", current_text, ellipsis);
            text_width_px =
                get_ft_text_width(&temp_text_with_ellipsis, current_font_size_pts, ft_face)?;

            let current_text_only_width_px =
                get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
            if current_text_only_width_px + ellipsis_width_px <= max_text_width_px {
                current_text = temp_text_with_ellipsis;
                text_width_px = get_ft_text_width(&current_text, current_font_size_pts, ft_face)?;
                break;
            }
        }

        if text_width_px > max_text_width_px {
            let mut temp_ellipsis = ellipsis.to_string();
            while get_ft_text_width(&temp_ellipsis, current_font_size_pts, ft_face)?
                > max_text_width_px
                && !temp_ellipsis.is_empty()
            {
                temp_ellipsis.pop();
            }
            current_text = temp_ellipsis;
            if current_text.starts_with(ellipsis.chars().next().unwrap_or_default())
                && current_text.len() < ellipsis.len()
            {
                truncated_chars = original_text.chars().count();
            } else if current_text.is_empty() {
                truncated_chars = original_text.chars().count();
            }
        }
    }

    Ok(TextCheckResult {
        final_font_size_pts: current_font_size_pts,
        truncated_chars,
        final_text: current_text,
    })
}

pub fn load_and_process_config(config_path: &str) -> Result<AppConfig, String> {
    let config_content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read configuration file '{}': {}", config_path, e))?;

    let mut app_config: AppConfig = toml::from_str(&config_content).map_err(|e| {
        format!(
            "Failed to parse TOML configuration from '{}': {}",
            config_path, e
        )
    })?;

    let mut keycode_resolution_errors = Vec::new();
    for key_conf in app_config.key.iter_mut() {
        if key_conf.width <= 0.0 {
            keycode_resolution_errors.push(format!(
                "Key '{}' has invalid width: {}",
                key_conf.name, key_conf.width
            ));
        }
        if key_conf.height <= 0.0 {
            keycode_resolution_errors.push(format!(
                "Key '{}' has invalid height: {}",
                key_conf.name, key_conf.height
            ));
        }

        let resolved_code = match key_conf.raw_keycode.as_ref() {
            Some(SerdeValue::String(s)) => keycodes::get_keycode_from_string(s),
            Some(SerdeValue::U8(i)) => Ok(*i as u32),
            Some(SerdeValue::U16(i)) => Ok(*i as u32),
            Some(SerdeValue::U32(i)) => Ok(*i),
            Some(SerdeValue::U64(i)) => {
                if *i <= u32::MAX as u64 {
                    Ok(*i as u32)
                } else {
                    Err(format!(
                        "Integer keycode {} for key '{}' is too large for u32.",
                        i, key_conf.name
                    ))
                }
            }
            Some(SerdeValue::I8(i)) => {
                if *i >= 0 {
                    Ok(*i as u32)
                } else {
                    Err(format!(
                        "Negative keycode {} for key '{}' is invalid.",
                        i, key_conf.name
                    ))
                }
            }
            Some(SerdeValue::I16(i)) => {
                if *i >= 0 {
                    Ok(*i as u32)
                } else {
                    Err(format!(
                        "Negative keycode {} for key '{}' is invalid.",
                        i, key_conf.name
                    ))
                }
            }
            Some(SerdeValue::I32(i)) => {
                if *i >= 0 {
                    Ok(*i as u32)
                } else {
                    Err(format!(
                        "Negative keycode {} for key '{}' is invalid.",
                        i, key_conf.name
                    ))
                }
            }
            Some(SerdeValue::I64(i)) => {
                if *i >= 0 && *i <= u32::MAX as i64 {
                    Ok(*i as u32)
                } else {
                    Err(format!(
                        "Integer keycode {} for key '{}' is out of valid u32 range.",
                        i, key_conf.name
                    ))
                }
            }
            None => keycodes::get_keycode_from_string(&key_conf.name),
            Some(other_type) => Err(format!(
                "Invalid type for keycode field for key '{}': expected string or integer, got {:?}",
                key_conf.name, other_type
            )),
        };

        match resolved_code {
            Ok(code) => key_conf.keycode = code,
            Err(e) => {
                let error_msg = if key_conf.raw_keycode.is_none() {
                    format!(
                        "Error processing key '{}': Could not resolve default keycode from name ('{}'). Please specify a 'keycode' field. Details: {}",
                        key_conf.name, key_conf.name, e
                    )
                } else {
                    format!(
                        "Error processing keycode for key '{}': {}",
                        key_conf.name, e
                    )
                };
                keycode_resolution_errors.push(error_msg);
            }
        }
    }

    if !keycode_resolution_errors.is_empty() {
        return Err(format!(
            "Errors found during keycode resolution:\n- {}",
            keycode_resolution_errors.join("\n- ")
        ));
    }

    Ok(app_config)
}
