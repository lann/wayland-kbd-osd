// Configuration loading and related structs
use serde::Deserialize;
use std::fs;

use crate::keycodes::{self, KeycodeRepr};

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
    pub raw_keycode: Option<KeycodeRepr>,
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

    let parse_hex_component = |hex_pair: &str, component_name: &str| {
        u8::from_str_radix(hex_pair, 16).map_err(|e| {
            format!(
                "Invalid hexadecimal value for {} component in color string '{}': '{}'. Error: {}",
                component_name, color_str, hex_pair, e
            )
        })
    };

    match s.len() {
        6 => { // RRGGBB
            let r = parse_hex_component(&s[0..2], "Red (RR)")?;
            let g = parse_hex_component(&s[2..4], "Green (GG)")?;
            let b = parse_hex_component(&s[4..6], "Blue (BB)")?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0)) // Default alpha to 1.0
        }
        8 => { // RRGGBBAA
            let r = parse_hex_component(&s[0..2], "Red (RR)")?;
            let g = parse_hex_component(&s[2..4], "Green (GG)")?;
            let b = parse_hex_component(&s[4..6], "Blue (BB)")?;
            let a = parse_hex_component(&s[6..8], "Alpha (AA)")?;
            Ok((
                r as f64 / 255.0,
                g as f64 / 255.0,
                b as f64 / 255.0,
                a as f64 / 255.0,
            ))
        }
        3 => { // RGB
            let r_char = s.chars().next().ok_or_else(|| "Empty string after '#' for RGB color".to_string())?;
            let g_char = s.chars().nth(1).ok_or_else(|| "Too short for G in RGB color".to_string())?;
            let b_char = s.chars().nth(2).ok_or_else(|| "Too short for B in RGB color".to_string())?;

            let r_str = format!("{}{}", r_char, r_char);
            let g_str = format!("{}{}", g_char, g_char);
            let b_str = format!("{}{}", b_char, b_char);

            let r = parse_hex_component(&r_str, "Red (R)")?;
            let g = parse_hex_component(&g_str, "Green (G)")?;
            let b = parse_hex_component(&b_str, "Blue (B)")?;
            Ok((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0))
        }
        4 => { // RGBA
            let r_char = s.chars().next().ok_or_else(|| "Empty string after '#' for RGBA color".to_string())?;
            let g_char = s.chars().nth(1).ok_or_else(|| "Too short for G in RGBA color".to_string())?;
            let b_char = s.chars().nth(2).ok_or_else(|| "Too short for B in RGBA color".to_string())?;
            let a_char = s.chars().nth(3).ok_or_else(|| "Too short for A in RGBA color".to_string())?;

            let r_str = format!("{}{}", r_char, r_char);
            let g_str = format!("{}{}", g_char, g_char);
            let b_str = format!("{}{}", b_char, b_char);
            let a_str = format!("{}{}", a_char, a_char);

            let r = parse_hex_component(&r_str, "Red (R)")?;
            let g = parse_hex_component(&g_str, "Green (G)")?;
            let b = parse_hex_component(&b_str, "Blue (B)")?;
            let a = parse_hex_component(&a_str, "Alpha (A)")?;
            Ok((
                r as f64 / 255.0,
                g as f64 / 255.0,
                b as f64 / 255.0,
                a as f64 / 255.0,
            ))
        }
        _ => Err(format!(
            "Invalid color string format for '{}'. Expected #RRGGBB, #RRGGBBAA, #RGB, or #RGBA. Length of '{}' (after #) is {}.",
            color_str, s, s.len()
        )),
    }
}

// Default appearance values (unscaled) - also used in AppState::draw
// Note: check_config related usage is now in src/check.rs
pub const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
pub const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
pub const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;
pub const DEFAULT_ROTATION_DEGREES: f32 = 0.0;

// TextCheckResult was moved to text_utils.rs as TextLayoutResult

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
        // Basic validation for key dimensions, moved here from main's --check logic
        // as it's fundamental to a valid key definition before resolving keycodes.
        if key_conf.width <= 0.0 {
            keycode_resolution_errors.push(format!(
                "Key '{}' has invalid width: {}. Width must be positive.",
                key_conf.name, key_conf.width
            ));
        }
        if key_conf.height <= 0.0 {
            keycode_resolution_errors.push(format!(
                "Key '{}' has invalid height: {}. Height must be positive.",
                key_conf.name, key_conf.height
            ));
        }

        // Resolve keycode:
        // The `raw_keycode` field in `KeyConfig` is an Option<SerdeValue>.
        // This allows the TOML to specify keycodes as strings (e.g., "a", "leftshift")
        // or as numbers (e.g., 30, 42).
        // If `raw_keycode` is None (i.e., the 'keycode' field is missing in TOML for this key),
        // we attempt to derive the keycode from the key's 'name' field.
        let resolved_code = match key_conf.raw_keycode.as_ref() {
            // Case 1: Keycode is specified as Text (string)
            Some(KeycodeRepr::Text(s)) => keycodes::get_keycode_from_string(s),
            // Case 2: Keycode is specified as a Number (u32)
            Some(KeycodeRepr::Number(n)) => Ok(*n),
            // Case 3: 'keycode' field is not specified in TOML for this key.
            // Attempt to resolve the keycode from the key's 'name' field (e.g. name = "A" -> keycode for A).
            None => keycodes::get_keycode_from_string(&key_conf.name),
        };

        match resolved_code {
            Ok(code) => key_conf.keycode = code, // Store the successfully resolved u32 keycode.
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
