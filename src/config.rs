// Configuration loading and related structs

//! This module defines the data structures for application configuration,
//! loaded from a TOML file. It includes parsing for colors, sizes,
//! and key definitions, along with default values and validation.

use serde::Deserialize;
use std::fs;

use crate::keycodes::{self, KeycodeRepr};

/// Represents a size dimension that can be specified in absolute pixels or as a ratio.
///
/// Used in TOML configuration for `size_width` and `size_height` of the overlay.
/// It can be deserialized from an integer (interpreted as pixels) or a float
/// (interpreted as a ratio, e.g., 0.5 for 50%).
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(untagged)] // Allows parsing "100" as Pixels(100) or "0.5" as Ratio(0.5)
pub enum SizeDimension {
    /// Size in absolute pixels.
    Pixels(u32),
    /// Size as a ratio (e.g., of screen width/height).
    Ratio(f32),
}

/// Enum for specifying the anchor position of the overlay on the screen.
///
/// Used in TOML as `overlay.position`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    /// Top edge, centered horizontally by default unless combined with Left/Right.
    Top,
    /// Bottom edge, centered horizontally by default unless combined with Left/Right.
    Bottom,
    /// Left edge, centered vertically by default unless combined with Top/Bottom.
    Left,
    Right,
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// Center vertically, aligned to the left edge.
    CenterLeft,
    /// Center vertically, aligned to the right edge.
    CenterRight,
}

/// Configuration for the OSD overlay window.
///
/// Defines properties like target screen, position, size, margins,
/// and default colors for various elements.
#[derive(Deserialize, Debug, Clone)]
pub struct OverlayConfig {
    /// Optional identifier for the target screen (e.g., "DP-1", "0").
    /// If `None`, the compositor chooses the screen.
    #[serde(default)]
    pub screen: Option<String>,
    /// Position of the overlay on the screen.
    #[serde(default = "default_overlay_position")]
    pub position: OverlayPosition,
    /// Optional width of the overlay. Can be pixels or ratio.
    pub size_width: Option<SizeDimension>,
    /// Optional height of the overlay. Can be pixels or ratio.
    pub size_height: Option<SizeDimension>,
    /// Top margin in pixels.
    #[serde(default = "default_overlay_margin")]
    pub margin_top: i32,
    /// Right margin in pixels.
    #[serde(default = "default_overlay_margin")]
    pub margin_right: i32,
    /// Bottom margin in pixels.
    #[serde(default = "default_overlay_margin")]
    pub margin_bottom: i32,
    /// Left margin in pixels.
    #[serde(default = "default_overlay_margin")]
    pub margin_left: i32,
    /// Background color of the OSD when no keys are pressed (e.g., fully transparent).
    #[serde(default = "default_background_color_inactive")]
    pub background_color_inactive: String,
    /// Background color of the OSD when one or more keys are pressed.
    /// (Currently unused for global background, key-specific active colors are used).
    #[serde(default = "default_background_color_active")]
    pub background_color_active: String,
    /// Default background color for keys in their normal (inactive) state.
    #[serde(default = "default_key_background_color_string")]
    pub default_key_background_color: String,
    /// Default text color for key labels.
    #[serde(default = "default_key_text_color_string")]
    pub default_key_text_color: String,
    /// Default color for key outlines/borders.
    #[serde(default = "default_key_outline_color_string")]
    pub default_key_outline_color: String,
    /// Default background color for keys when they are active (pressed).
    #[serde(default = "default_active_key_background_color_string")]
    pub active_key_background_color: String,
    /// Default text color for key labels when they are active (pressed).
    #[serde(default = "default_active_key_text_color_string")]
    pub active_key_text_color: String,
}

/// Returns the default `OverlayPosition` (`BottomCenter`).
fn default_overlay_position() -> OverlayPosition {
    OverlayPosition::BottomCenter
}
/// Returns the default margin value (`0`).
fn default_overlay_margin() -> i32 {
    0
}
/// Returns the default inactive background color string (`"#00000000"`).
fn default_background_color_inactive() -> String {
    "#00000000".to_string()
}
/// Returns the default active background color string (`"#A0A0A0D0"`).
fn default_background_color_active() -> String {
    "#A0A0A0D0".to_string()
}
/// Returns the default key background color string (`"#4D4D4D80"`).
pub fn default_key_background_color_string() -> String {
    "#4D4D4D80".to_string()
}
/// Returns the default key text color string (`"#B3B3B3CC"`).
fn default_key_text_color_string() -> String {
    "#B3B3B3CC".to_string()
}
/// Returns the default key outline color string (`"#B3B3B3CC"`).
fn default_key_outline_color_string() -> String {
    "#B3B3B3CC".to_string()
}
/// Returns the default active key background color string (`"#A0A0F0FF"`).
fn default_active_key_background_color_string() -> String {
    "#A0A0F0FF".to_string()
}
/// Returns the default active key text color string (same as normal text color).
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

/// Configuration for a single key displayed on the OSD.
///
/// Defines the key's appearance (name, dimensions, position, colors) and
/// its corresponding input keycode.
#[derive(Deserialize, Debug, Clone)]
pub struct KeyConfig {
    /// Display name or label for the key (e.g., "A", "Shift").
    pub name: String,
    /// Width of the key in abstract layout units.
    pub width: f32,
    /// Height of the key in abstract layout units.
    pub height: f32,
    /// X-coordinate of the top-left corner in abstract layout units.
    pub left: f32,
    /// Y-coordinate of the top-left corner in abstract layout units.
    pub top: f32,
    /// Raw keycode representation from TOML (string or number).
    /// This is processed into the `keycode` field.
    #[serde(alias = "keycode")]
    pub raw_keycode: Option<KeycodeRepr>,
    /// Resolved numerical keycode (e.g., from `linux/input-event-codes.h`).
    /// This field is populated by `load_and_process_config`.
    #[serde(skip_deserializing)]
    pub keycode: u32,
    /// Optional rotation of the key in degrees.
    pub rotation_degrees: Option<f32>,
    /// Optional custom text size for this key (unscaled points).
    pub text_size: Option<f32>,
    /// Optional custom corner radius for this key (unscaled).
    pub corner_radius: Option<f32>,
    /// Optional custom border thickness for this key (unscaled).
    pub border_thickness: Option<f32>,
    /// Optional custom background color string for this key.
    pub background_color: Option<String>,
}

/// Root structure for the application configuration.
///
/// Contains a list of `KeyConfig` definitions and an `OverlayConfig`.
#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    /// Vector of key configurations.
    #[serde(default)]
    pub key: Vec<KeyConfig>,
    /// Configuration for the overlay window.
    #[serde(default)]
    pub overlay: OverlayConfig,
}

/// Parses a color string into a tuple of (r, g, b, a) components.
///
/// Supports formats:
/// - `"#RRGGBB"` (e.g., `"#FF0000"` for red)
/// - `"#RRGGBBAA"` (e.g., `"#FF000080"` for semi-transparent red)
/// - `"#RGB"` (e.g., `"#F00"` for red, equivalent to `"#FF0000"`)
/// - `"#RGBA"` (e.g., `"#F008"` for semi-transparent red, equivalent to `"#FF000088"`)
///
/// Color components are returned as `f64` values between 0.0 and 1.0.
///
/// # Arguments
///
/// * `color_str` - The color string to parse.
///
/// # Returns
///
/// * `Ok((f64, f64, f64, f64))` representing (r, g, b, a).
/// * `Err(String)` if the color string is invalid.
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

/// Default corner radius for keys, in unscaled layout units.
pub const DEFAULT_CORNER_RADIUS_UNSCALED: f32 = 8.0;
/// Default border thickness for keys, in unscaled layout units.
pub const DEFAULT_BORDER_THICKNESS_UNSCALED: f32 = 2.0;
/// Default text size for key labels, in unscaled points.
pub const DEFAULT_TEXT_SIZE_UNSCALED: f32 = 18.0;
/// Default rotation for keys, in degrees.
pub const DEFAULT_ROTATION_DEGREES: f32 = 0.0;

/// Loads and processes the application configuration from a TOML file.
///
/// This function reads the specified TOML file, deserializes it into an
/// `AppConfig` struct, and then processes each `KeyConfig` to:
/// 1. Validate that key width and height are positive.
/// 2. Resolve the `raw_keycode` (which can be a string name or a number from TOML)
///    into a numerical `keycode`. If `raw_keycode` is not specified, it attempts
///    to derive the keycode from the key's `name` field.
///
/// # Arguments
///
/// * `config_path` - Path to the TOML configuration file.
///
/// # Returns
///
/// * `Ok(AppConfig)` if loading and processing are successful.
/// * `Err(String)` with a descriptive error message if any part of the process fails
///   (file reading, TOML parsing, key validation, or keycode resolution).
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
