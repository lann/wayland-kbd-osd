// Based on Linux input-event-codes.h
// Fetched from: https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/input-event-codes.h

/// Resolves a key name string (e.g., "leftshift", "a", "KP_Enter") to its Linux keycode.
///
/// ## Normalization Rules:
/// Before matching, the input `key_name_str` undergoes normalization:
/// 1. **Lowercase:** The entire string is converted to lowercase.
/// 2. **Underscore Removal:** All underscores (`_`) are removed.
///    (e.g., "keypad_enter" becomes "keypadenter").
/// 3. **Hyphen Removal (Conditional):** Hyphens (`-`) are removed ONLY IF the string
///    contains at least one alphabetic character. This heuristic aims to preserve
///    hyphens in symbolic key names (like "-") while allowing hyphens in aliases
///    (e.g., "volume-down" becomes "volumedown").
///    Single character symbols like "-" or "=" will not have their hyphen/equal sign removed.
///
/// ## Ambiguous Keys:
/// Certain common short names for modifier keys are considered ambiguous and will result
/// in an error. Users must specify the exact version (e.g., "leftshift" instead of "shift").
/// The ambiguous names checked are:
/// - "shift"
/// - "ctrl", "control"
/// - "alt"
/// - "meta", "win", "windows", "super"
///
/// The function then attempts to match the normalized string against a comprehensive list
/// of known key names and their corresponding Linux event codes.
pub fn get_keycode_from_string(key_name_str: &str) -> Result<u32, String> {
    let mut normalized_key_name = key_name_str.to_lowercase();

    // Rule 2: Replace underscores globally
    normalized_key_name = normalized_key_name.replace("_", "");

    // Rule 3: Replace hyphens only if the string is likely an alias rather than a symbol key itself.
    if normalized_key_name.chars().any(char::is_alphabetic) {
        normalized_key_name = normalized_key_name.replace("-", "");
    }

    // Handle ambiguous keys first (using the already partially normalized name)
    match normalized_key_name.as_str() {
        "shift" => return Err(
            "Ambiguous key name 'shift'. Please specify 'leftshift' or 'rightshift'.".to_string()
        ),
        "ctrl" | "control" => return Err(
            "Ambiguous key name 'ctrl' or 'control'. Please specify 'leftctrl' or 'rightctrl'.".to_string()
        ),
        "alt" => return Err(
            "Ambiguous key name 'alt'. Please specify 'leftalt' or 'rightalt' (or 'altgr' for Right Alt / AltGr).".to_string()
        ),
        "meta" | "win" | "windows" | "super" => return Err(
            "Ambiguous key name like 'meta', 'win', 'super'. Please specify 'leftmeta' (or 'lwin', 'lsuper') or 'rightmeta' (or 'rwin', 'rsuper').".to_string()
        ),
        _ => {} // Not an ambiguous key, proceed to main match
    }

    // Match against the fully normalized key name.
    match normalized_key_name.as_str() {
        // Standard Keys from input-event-codes.h
        "esc" | "escape" => Ok(1),
        "1" => Ok(2),
        "2" => Ok(3),
        "3" => Ok(4),
        "4" => Ok(5),
        "5" => Ok(6),
        "6" => Ok(7),
        "7" => Ok(8),
        "8" => Ok(9),
        "9" => Ok(10),
        "0" => Ok(11),
        "minus" | "-" => Ok(12),
        "equal" | "=" => Ok(13),
        "backspace" | "bksp" => Ok(14),
        "tab" => Ok(15),
        "q" => Ok(16),
        "w" => Ok(17),
        "e" => Ok(18),
        "r" => Ok(19),
        "t" => Ok(20),
        "y" => Ok(21),
        "u" => Ok(22),
        "i" => Ok(23),
        "o" => Ok(24),
        "p" => Ok(25),
        "leftbrace" | "[" | "lbracket" => Ok(26),
        "rightbrace" | "]" | "rbracket" => Ok(27),
        "enter" | "return" => Ok(28), // KEY_ENTER
        "leftctrl" | "lctrl" => Ok(29),
        "a" => Ok(30),
        "s" => Ok(31),
        "d" => Ok(32),
        "f" => Ok(33),
        "g" => Ok(34),
        "h" => Ok(35),
        "j" => Ok(36),
        "k" => Ok(37),
        "l" => Ok(38),
        "semicolon" | ";" => Ok(39),
        "apostrophe" | "'" | "quote" => Ok(40),
        "grave" | "`" | "tilde" => Ok(41),
        "leftshift" | "lshift" => Ok(42),
        "backslash" | "\\" => Ok(43),
        "z" => Ok(44),
        "x" => Ok(45),
        "c" => Ok(46),
        "v" => Ok(47),
        "b" => Ok(48),
        "n" => Ok(49),
        "m" => Ok(50),
        "comma" | "," => Ok(51),
        "dot" | "." | "period" => Ok(52),
        "slash" | "/" => Ok(53),
        "rightshift" | "rshift" => Ok(54),
        "kpasterisk" | "keypadasterisk" | "kpmultiply" | "kpmul" => Ok(55),
        "leftalt" | "lalt" => Ok(56),
        "space" => Ok(57),
        "capslock" | "caps" => Ok(58),
        "f1" => Ok(59),
        "f2" => Ok(60),
        "f3" => Ok(61),
        "f4" => Ok(62),
        "f5" => Ok(63),
        "f6" => Ok(64),
        "f7" => Ok(65),
        "f8" => Ok(66),
        "f9" => Ok(67),
        "f10" => Ok(68),
        "numlock" | "num" => Ok(69),
        "scrolllock" | "scroll" => Ok(70),
        "kp7" | "keypad7" => Ok(71),
        "kp8" | "keypad8" => Ok(72),
        "kp9" | "keypad9" => Ok(73),
        "kpminus" | "keypadminus" | "kpsubtract" | "kpsub" => Ok(74),
        "kp4" | "keypad4" => Ok(75),
        "kp5" | "keypad5" => Ok(76),
        "kp6" | "keypad6" => Ok(77),
        "kpplus" | "keypadplus" | "kpadd" => Ok(78),
        "kp1" | "keypad1" => Ok(79),
        "kp2" | "keypad2" => Ok(80),
        "kp3" | "keypad3" => Ok(81),
        "kp0" | "keypad0" => Ok(82),
        "kpdot" | "keypaddot" | "kpdecimal" | "kpperiod" => Ok(83),

        "zenkakuhankaku" => Ok(85),
        "102nd" => Ok(86), // Often a second backslash or <>| key on ISO layouts
        "f11" => Ok(87),
        "f12" => Ok(88),
        "ro" => Ok(89), // Japanese Yen/Ro key
        "katakana" => Ok(90),
        "hiragana" => Ok(91),
        "henkan" => Ok(92),
        "katakanahiragana" => Ok(93),
        "muhenkan" => Ok(94),
        "kpjpcomma" | "keypadjpcomma" => Ok(95), // Japanese keypad comma
        "kpenter" | "keypadenter" => Ok(96),
        "rightctrl" | "rctrl" => Ok(97),
        "kpslash" | "keypadslash" | "kpdivide" | "kpdiv" => Ok(98),
        "sysrq" | "printscreen" | "prtscr" => Ok(99),
        "rightalt" | "ralt" | "altgr" => Ok(100),
        "linefeed" => Ok(101), // Not common on PC keyboards
        "home" => Ok(102),
        "up" | "uparrow" => Ok(103),
        "pageup" | "pgup" => Ok(104),
        "left" | "leftarrow" => Ok(105),
        "right" | "rightarrow" => Ok(106),
        "end" => Ok(107),
        "down" | "downarrow" => Ok(108),
        "pagedown" | "pgdn" => Ok(109),
        "insert" | "ins" => Ok(110),
        "delete" | "del" => Ok(111),
        "mute" => Ok(113),
        "volumedown" => Ok(114),
        "volumeup" => Ok(115),
        "power" => Ok(116), // SC System Power Down
        "kpequal" | "keypadequal" => Ok(117),
        "kpplusminus" | "keypadplusminus" => Ok(118),
        "pause" | "pausebreak" => Ok(119),
        "kpcomma" | "keypadcomma" => Ok(121), // Often on Brazilian/some European layouts
        "hanja" => Ok(123),                   // Korean Hanja
        "yen" => Ok(124),                     // Japanese Yen (sometimes different from Ro)
        "leftmeta" | "lmeta" | "leftwindows" | "lwin" | "leftsuper" | "lsuper" => Ok(125),
        "rightmeta" | "rmeta" | "rightwindows" | "rwin" | "rightsuper" | "rsuper" => Ok(126),
        "compose" => Ok(127), // Compose key

        // Media keys (selection, more might be needed if desired)
        "stop" => Ok(128), // AC Stop
        "again" => Ok(129),
        "props" => Ok(130), // AC Properties
        "undo" => Ok(131),  // AC Undo
        "front" => Ok(132),
        "copy" => Ok(133),                // AC Copy
        "open" => Ok(134),                // AC Open
        "paste" => Ok(135),               // AC Paste
        "find" => Ok(136),                // AC Search
        "cut" => Ok(137),                 // AC Cut
        "help" => Ok(138),                // AL Integrated Help Center
        "menu" | "appmenu" => Ok(139),    // Menu key (application menu)
        "calc" | "calculator" => Ok(140), // AL Calculator
        "setup" => Ok(141),
        "sleep" => Ok(142),  // SC System Sleep
        "wakeup" => Ok(143), // System Wake Up
        "file" => Ok(144),   // AL Local Machine Browser
        "www" => Ok(150),    // AL Internet Browser
        "mail" => Ok(155),
        "bookmarks" => Ok(156), // AC Bookmarks
        "computer" => Ok(157),
        "back" => Ok(158),    // AC Back
        "forward" => Ok(159), // AC Forward
        "ejectcd" | "eject" => Ok(161),
        "nextsong" => Ok(163),
        "playpause" => Ok(164),
        "previoussong" => Ok(165),
        "stopcd" => Ok(166), // Different from KEY_STOP
        "record" => Ok(167),
        "rewind" => Ok(168),
        "phone" => Ok(169),
        "homepage" => Ok(172), // AC Home
        "refresh" => Ok(173),  // AC Refresh
        "exit" => Ok(174),     // AC Exit
        "scrollup" => Ok(177),
        "scrolldown" => Ok(178),
        "kpleftparen" | "keypadleftparen" => Ok(179),
        "kprightparen" | "keypadrightparen" => Ok(180),

        // F13-F24 (less common on physical keyboards but exist)
        "f13" => Ok(183),
        "f14" => Ok(184),
        "f15" => Ok(185),
        "f16" => Ok(186),
        "f17" => Ok(187),
        "f18" => Ok(188),
        "f19" => Ok(189),
        "f20" => Ok(190),
        "f21" => Ok(191),
        "f22" => Ok(192),
        "f23" => Ok(193),
        "f24" => Ok(194),

        "playcd" => Ok(200),
        "pausecd" => Ok(201),
        "print" => Ok(210), // AC Print (different from PrtSc/SysRq)
        "camera" => Ok(212),
        "search" => Ok(217), // Often a dedicated search key
        "brightnessdown" => Ok(224),
        "brightnessup" => Ok(225),
        "kbdillumtoggle" | "keyboardilluminationtoggle" => Ok(228),
        "kbdillumdown" | "keyboardilluminationdown" => Ok(229),
        "kbdillumup" | "keyboardilluminationup" => Ok(230),
        "micmute" => Ok(248),

        "fn" => Ok(0x1d0),    // 464
        "fnesc" => Ok(0x1d1), // 465
        _ => Err(format!("Unknown key name: '{}'", key_name_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_keys() {
        assert_eq!(get_keycode_from_string("q").unwrap(), 16);
        assert_eq!(get_keycode_from_string("Q").unwrap(), 16);
        assert_eq!(get_keycode_from_string("esc").unwrap(), 1);
        assert_eq!(get_keycode_from_string("ESC").unwrap(), 1);
        assert_eq!(get_keycode_from_string("enter").unwrap(), 28);
        assert_eq!(get_keycode_from_string("RETURN").unwrap(), 28);
        assert_eq!(get_keycode_from_string("space").unwrap(), 57);
    }

    #[test]
    fn test_number_keys() {
        assert_eq!(get_keycode_from_string("1").unwrap(), 2);
        assert_eq!(get_keycode_from_string("0").unwrap(), 11);
    }

    #[test]
    fn test_f_keys() {
        assert_eq!(get_keycode_from_string("f1").unwrap(), 59);
        assert_eq!(get_keycode_from_string("F1").unwrap(), 59);
        assert_eq!(get_keycode_from_string("f12").unwrap(), 88);
        assert_eq!(get_keycode_from_string("f13").unwrap(), 183);
        assert_eq!(get_keycode_from_string("f24").unwrap(), 194);
    }

    #[test]
    fn test_modifier_keys() {
        assert_eq!(get_keycode_from_string("leftshift").unwrap(), 42);
        assert_eq!(get_keycode_from_string("lshift").unwrap(), 42);
        assert_eq!(get_keycode_from_string("rightshift").unwrap(), 54);
        assert_eq!(get_keycode_from_string("rshift").unwrap(), 54);
        assert_eq!(get_keycode_from_string("leftctrl").unwrap(), 29);
        assert_eq!(get_keycode_from_string("lctrl").unwrap(), 29);
        assert_eq!(get_keycode_from_string("rightctrl").unwrap(), 97);
        assert_eq!(get_keycode_from_string("rctrl").unwrap(), 97);
        assert_eq!(get_keycode_from_string("leftalt").unwrap(), 56);
        assert_eq!(get_keycode_from_string("lalt").unwrap(), 56);
        assert_eq!(get_keycode_from_string("rightalt").unwrap(), 100);
        assert_eq!(get_keycode_from_string("ralt").unwrap(), 100);
        assert_eq!(get_keycode_from_string("altgr").unwrap(), 100);
        assert_eq!(get_keycode_from_string("leftmeta").unwrap(), 125);
        assert_eq!(get_keycode_from_string("lmeta").unwrap(), 125);
        assert_eq!(get_keycode_from_string("lwin").unwrap(), 125);
        assert_eq!(get_keycode_from_string("leftsuper").unwrap(), 125);
        assert_eq!(get_keycode_from_string("rightmeta").unwrap(), 126);
        assert_eq!(get_keycode_from_string("rmeta").unwrap(), 126);
        assert_eq!(get_keycode_from_string("rwin").unwrap(), 126);
        assert_eq!(get_keycode_from_string("rightsuper").unwrap(), 126);
    }

    #[test]
    fn test_keypad_keys() {
        assert_eq!(get_keycode_from_string("kp0").unwrap(), 82);
        assert_eq!(get_keycode_from_string("keypad0").unwrap(), 82);
        assert_eq!(get_keycode_from_string("kp_0").unwrap(), 82); // Test underscore normalization
        assert_eq!(get_keycode_from_string("kp7").unwrap(), 71);
        assert_eq!(get_keycode_from_string("keypad7").unwrap(), 71);
        assert_eq!(get_keycode_from_string("kpenter").unwrap(), 96);
        assert_eq!(get_keycode_from_string("keypadenter").unwrap(), 96);
        assert_eq!(get_keycode_from_string("kpslash").unwrap(), 98);
        assert_eq!(get_keycode_from_string("kpdivide").unwrap(), 98);
        assert_eq!(get_keycode_from_string("kpasterisk").unwrap(), 55);
        assert_eq!(get_keycode_from_string("kpmultiply").unwrap(), 55);
        assert_eq!(get_keycode_from_string("kpminus").unwrap(), 74);
        assert_eq!(get_keycode_from_string("kpsub").unwrap(), 74);
        assert_eq!(get_keycode_from_string("kpplus").unwrap(), 78);
        assert_eq!(get_keycode_from_string("kpadd").unwrap(), 78);
        assert_eq!(get_keycode_from_string("kpdot").unwrap(), 83);
        assert_eq!(get_keycode_from_string("kpdecimal").unwrap(), 83);
        assert_eq!(get_keycode_from_string("kpequal").unwrap(), 117);
        assert_eq!(get_keycode_from_string("kpcomma").unwrap(), 121);
        assert_eq!(get_keycode_from_string("kpjpcomma").unwrap(), 95);
        assert_eq!(get_keycode_from_string("kpplusminus").unwrap(), 118);
        assert_eq!(get_keycode_from_string("kpleftparen").unwrap(), 179);
        assert_eq!(get_keycode_from_string("kprightparen").unwrap(), 180);
    }

    #[test]
    fn test_symbol_keys() {
        assert_eq!(get_keycode_from_string(".").unwrap(), 52);
        assert_eq!(get_keycode_from_string("period").unwrap(), 52);
        assert_eq!(get_keycode_from_string(",").unwrap(), 51);
        assert_eq!(get_keycode_from_string("comma").unwrap(), 51);
        assert_eq!(get_keycode_from_string("/").unwrap(), 53);
        assert_eq!(get_keycode_from_string("slash").unwrap(), 53);
        assert_eq!(get_keycode_from_string(";").unwrap(), 39);
        assert_eq!(get_keycode_from_string("semicolon").unwrap(), 39);
        assert_eq!(get_keycode_from_string("'").unwrap(), 40);
        assert_eq!(get_keycode_from_string("apostrophe").unwrap(), 40);
        assert_eq!(get_keycode_from_string("quote").unwrap(), 40);
        assert_eq!(get_keycode_from_string("[").unwrap(), 26);
        assert_eq!(get_keycode_from_string("leftbrace").unwrap(), 26);
        assert_eq!(get_keycode_from_string("lbracket").unwrap(), 26);
        assert_eq!(get_keycode_from_string("]").unwrap(), 27);
        assert_eq!(get_keycode_from_string("rightbrace").unwrap(), 27);
        assert_eq!(get_keycode_from_string("rbracket").unwrap(), 27);
        assert_eq!(get_keycode_from_string("\\").unwrap(), 43);
        assert_eq!(get_keycode_from_string("backslash").unwrap(), 43);
        assert_eq!(get_keycode_from_string("`").unwrap(), 41);
        assert_eq!(get_keycode_from_string("grave").unwrap(), 41);
        assert_eq!(get_keycode_from_string("tilde").unwrap(), 41);
        assert_eq!(get_keycode_from_string("-").unwrap(), 12);
        assert_eq!(get_keycode_from_string("minus").unwrap(), 12);
        assert_eq!(get_keycode_from_string("=").unwrap(), 13);
        assert_eq!(get_keycode_from_string("equal").unwrap(), 13);
    }

    #[test]
    fn test_navigation_keys() {
        assert_eq!(get_keycode_from_string("home").unwrap(), 102);
        assert_eq!(get_keycode_from_string("end").unwrap(), 107);
        assert_eq!(get_keycode_from_string("pageup").unwrap(), 104);
        assert_eq!(get_keycode_from_string("pgup").unwrap(), 104);
        assert_eq!(get_keycode_from_string("pagedown").unwrap(), 109);
        assert_eq!(get_keycode_from_string("pgdn").unwrap(), 109);
        assert_eq!(get_keycode_from_string("insert").unwrap(), 110);
        assert_eq!(get_keycode_from_string("ins").unwrap(), 110);
        assert_eq!(get_keycode_from_string("delete").unwrap(), 111);
        assert_eq!(get_keycode_from_string("del").unwrap(), 111);
        assert_eq!(get_keycode_from_string("up").unwrap(), 103);
        assert_eq!(get_keycode_from_string("uparrow").unwrap(), 103);
        assert_eq!(get_keycode_from_string("down").unwrap(), 108);
        assert_eq!(get_keycode_from_string("downarrow").unwrap(), 108);
        assert_eq!(get_keycode_from_string("left").unwrap(), 105);
        assert_eq!(get_keycode_from_string("leftarrow").unwrap(), 105);
        assert_eq!(get_keycode_from_string("right").unwrap(), 106);
        assert_eq!(get_keycode_from_string("rightarrow").unwrap(), 106);
    }

    #[test]
    fn test_other_common_keys() {
        assert_eq!(get_keycode_from_string("tab").unwrap(), 15);
        assert_eq!(get_keycode_from_string("backspace").unwrap(), 14);
        assert_eq!(get_keycode_from_string("bksp").unwrap(), 14);
        assert_eq!(get_keycode_from_string("capslock").unwrap(), 58);
        assert_eq!(get_keycode_from_string("caps").unwrap(), 58);
        assert_eq!(get_keycode_from_string("numlock").unwrap(), 69);
        assert_eq!(get_keycode_from_string("num").unwrap(), 69);
        assert_eq!(get_keycode_from_string("scrolllock").unwrap(), 70);
        assert_eq!(get_keycode_from_string("scroll").unwrap(), 70);
        assert_eq!(get_keycode_from_string("sysrq").unwrap(), 99);
        assert_eq!(get_keycode_from_string("printscreen").unwrap(), 99);
        assert_eq!(get_keycode_from_string("prtscr").unwrap(), 99);
        assert_eq!(get_keycode_from_string("pause").unwrap(), 119);
        assert_eq!(get_keycode_from_string("pausebreak").unwrap(), 119);
        assert_eq!(get_keycode_from_string("menu").unwrap(), 139);
        assert_eq!(get_keycode_from_string("appmenu").unwrap(), 139);
        assert_eq!(get_keycode_from_string("compose").unwrap(), 127);
        assert_eq!(get_keycode_from_string("fn").unwrap(), 0x1d0);
        assert_eq!(get_keycode_from_string("fnesc").unwrap(), 0x1d1);
    }

    #[test]
    fn test_media_keys() {
        assert_eq!(get_keycode_from_string("mute").unwrap(), 113);
        assert_eq!(get_keycode_from_string("volumedown").unwrap(), 114);
        assert_eq!(get_keycode_from_string("volumeup").unwrap(), 115);
        assert_eq!(get_keycode_from_string("playpause").unwrap(), 164);
        assert_eq!(get_keycode_from_string("stopcd").unwrap(), 166); // Note: KEY_STOP (128) is different
        assert_eq!(get_keycode_from_string("nextsong").unwrap(), 163);
        assert_eq!(get_keycode_from_string("previoussong").unwrap(), 165);
        assert_eq!(get_keycode_from_string("eject").unwrap(), 161);
        assert_eq!(get_keycode_from_string("ejectcd").unwrap(), 161);
        assert_eq!(get_keycode_from_string("micmute").unwrap(), 248);
    }

    #[test]
    fn test_unknown_key() {
        assert!(get_keycode_from_string("thiskeydoesnotexist").is_err());
        assert_eq!(
            get_keycode_from_string("thiskeydoesnotexist").unwrap_err(),
            "Unknown key name: 'thiskeydoesnotexist'"
        );
    }

    #[test]
    fn test_ambiguous_keys() {
        assert_eq!(
            get_keycode_from_string("shift").unwrap_err(),
            "Ambiguous key name 'shift'. Please specify 'leftshift' or 'rightshift'."
        );
        assert_eq!(
            get_keycode_from_string("SHIFT").unwrap_err(), // Test case insensitivity for ambiguous check
            "Ambiguous key name 'shift'. Please specify 'leftshift' or 'rightshift'."
        );
        assert_eq!(
            get_keycode_from_string("ctrl").unwrap_err(),
            "Ambiguous key name 'ctrl' or 'control'. Please specify 'leftctrl' or 'rightctrl'."
        );
        assert_eq!(
            get_keycode_from_string("control").unwrap_err(),
            "Ambiguous key name 'ctrl' or 'control'. Please specify 'leftctrl' or 'rightctrl'."
        );
        assert_eq!(
            get_keycode_from_string("alt").unwrap_err(),
            "Ambiguous key name 'alt'. Please specify 'leftalt' or 'rightalt' (or 'altgr' for Right Alt / AltGr)."
        );
        assert_eq!(
            get_keycode_from_string("meta").unwrap_err(),
            "Ambiguous key name like 'meta', 'win', 'super'. Please specify 'leftmeta' (or 'lwin', 'lsuper') or 'rightmeta' (or 'rwin', 'rsuper')."
        );
        assert_eq!(
            get_keycode_from_string("win").unwrap_err(),
            "Ambiguous key name like 'meta', 'win', 'super'. Please specify 'leftmeta' (or 'lwin', 'lsuper') or 'rightmeta' (or 'rwin', 'rsuper')."
        );
        assert_eq!(
            get_keycode_from_string("super").unwrap_err(),
            "Ambiguous key name like 'meta', 'win', 'super'. Please specify 'leftmeta' (or 'lwin', 'lsuper') or 'rightmeta' (or 'rwin', 'rsuper')."
        );
    }

    #[test]
    fn test_case_insensitivity_and_normalization() {
        assert_eq!(get_keycode_from_string("LeFt_ShIfT").unwrap(), 42);
        assert_eq!(get_keycode_from_string("LSHIFT").unwrap(), 42);
        assert_eq!(get_keycode_from_string("KeyPad_eNTeR").unwrap(), 96);
        assert_eq!(get_keycode_from_string("KPENTER").unwrap(), 96);
        assert_eq!(get_keycode_from_string("volume-down").unwrap(), 114); // Test dash normalization
    }
}
