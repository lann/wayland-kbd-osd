import toml

keys = []

# Constants for layout
u = 50.0  # 1U size
s = 5.0   # spacing

# Key dimensions
w_1u = u
w_1_25u = 1.25 * u
w_1_5u = 1.5 * u
w_1_75u = 1.75 * u
w_2u = 2.0 * u
w_2_25u = 2.25 * u
w_2_75u = 2.75 * u
w_6_25u = 6.25 * u
h_1u = u

# Row top coordinates
top_fn_row = 0.0
top_num_row = top_fn_row + h_1u + s
top_qwerty_row = top_num_row + h_1u + s
top_asdf_row = top_qwerty_row + h_1u + s
top_zxcv_row = top_asdf_row + h_1u + s
top_bottom_row = top_zxcv_row + h_1u + s

# Helper function to add keys
def add_key(name, left, top, width, height, keycode):
    keys.append({
        "name": name,
        "left": left,
        "top": top,
        "width": width,
        "height": height,
        "keycode": keycode
    })

# --- Function Row ---
current_x = 0.0
add_key("Esc", current_x, top_fn_row, w_1u, h_1u, "esc")
current_x += w_1u + s + 25.0 # Extra 25 gap

f_keys = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"]
for i, f_key_name in enumerate(f_keys):
    add_key(f_key_name, current_x, top_fn_row, w_1u, h_1u, f_key_name.lower())
    current_x += w_1u + s
    if i == 3 or i == 7: # Gaps after F4 and F8
        current_x += 25.0


# --- Number Row ---
current_x = 0.0
num_row_keys = [
    ("`", "grave", w_1u), ("1", "1", w_1u), ("2", "2", w_1u), ("3", "3", w_1u),
    ("4", "4", w_1u), ("5", "5", w_1u), ("6", "6", w_1u), ("7", "7", w_1u),
    ("8", "8", w_1u), ("9", "9", w_1u), ("0", "0", w_1u), ("-", "minus", w_1u),
    ("=", "equal", w_1u), ("Backspace", "backspace", w_2u)
]
for name, keycode, width in num_row_keys:
    add_key(name, current_x, top_num_row, width, h_1u, keycode)
    current_x += width + s

# --- QWERTY Row ---
current_x = 0.0
qwerty_row_keys = [
    ("Tab", "tab", w_1_5u), ("Q", "q", w_1u), ("W", "w", w_1u), ("E", "e", w_1u),
    ("R", "r", w_1u), ("T", "t", w_1u), ("Y", "y", w_1u), ("U", "u", w_1u),
    ("I", "i", w_1u), ("O", "o", w_1u), ("P", "p", w_1u), ("[", "leftbrace", w_1u),
    ("]", "rightbrace", w_1u), ("\\", "backslash", w_1_5u)
]
for name, keycode, width in qwerty_row_keys:
    add_key(name, current_x, top_qwerty_row, width, h_1u, keycode)
    current_x += width + s

# --- ASDF Row ---
current_x = 0.0
asdf_row_keys = [
    ("Caps Lock", "capslock", w_1_75u), ("A", "a", w_1u), ("S", "s", w_1u),
    ("D", "d", w_1u), ("F", "f", w_1u), ("G", "g", w_1u), ("H", "h", w_1u),
    ("J", "j", w_1u), ("K", "k", w_1u), ("L", "l", w_1u), (";", "semicolon", w_1u),
    ("'", "apostrophe", w_1u), ("Enter", "enter", w_2_25u)
]
for name, keycode, width in asdf_row_keys:
    add_key(name, current_x, top_asdf_row, width, h_1u, keycode)
    current_x += width + s

# --- ZXCV Row ---
current_x = 0.0
zxcv_row_keys = [
    ("Shift", "leftshift", w_2_25u), ("Z", "z", w_1u), ("X", "x", w_1u),
    ("C", "c", w_1u), ("V", "v", w_1u), ("B", "b", w_1u), ("N", "n", w_1u),
    ("M", "m", w_1u), (",", "comma", w_1u), (".", "dot", w_1u),
    ("/", "slash", w_1u), ("Shift", "rightshift", w_2_75u)
]
for name, keycode, width in zxcv_row_keys:
    add_key(name, current_x, top_zxcv_row, width, h_1u, keycode)
    current_x += width + s

# --- Bottom Row ---
current_x = 0.0
bottom_row_keys = [
    ("Ctrl", "leftctrl", w_1_25u), ("Super", "leftmeta", w_1_25u),
    ("Alt", "leftalt", w_1_25u), ("Space", "space", w_6_25u),
    ("Alt", "rightalt", w_1_25u), ("Super", "rightmeta", w_1_25u), # Or "FN"
    ("Menu", "menu", w_1_25u), ("Ctrl", "rightctrl", w_1_25u)
]
for name, keycode, width in bottom_row_keys:
    add_key(name, current_x, top_bottom_row, width, h_1u, keycode)
    current_x += width + s

# --- Navigation Cluster ---
# Max width of main block (F12 ends at 735+50 = 785)
# Backspace ends at 715+100 = 815
# Enter ends at 697.5 + 112.5 = 810
# RShift ends at 667.5 + 137.5 = 805
main_block_max_x = 815.0
nav_cluster_start_x = main_block_max_x + s + 25.0 # 25 unit gap

add_key("Print Screen", nav_cluster_start_x, top_fn_row, w_1u, h_1u, "printscreen")
add_key("Scroll Lock", nav_cluster_start_x + w_1u + s, top_fn_row, w_1u, h_1u, "scrolllock")
add_key("Pause", nav_cluster_start_x + (w_1u + s) * 2, top_fn_row, w_1u, h_1u, "pause")

add_key("Ins", nav_cluster_start_x, top_num_row, w_1u, h_1u, "insert")
add_key("Home", nav_cluster_start_x + w_1u + s, top_num_row, w_1u, h_1u, "home")
add_key("PgUp", nav_cluster_start_x + (w_1u + s) * 2, top_num_row, w_1u, h_1u, "pageup")

add_key("Del", nav_cluster_start_x, top_qwerty_row, w_1u, h_1u, "delete")
add_key("End", nav_cluster_start_x + w_1u + s, top_qwerty_row, w_1u, h_1u, "end")
add_key("PgDn", nav_cluster_start_x + (w_1u + s) * 2, top_qwerty_row, w_1u, h_1u, "pagedown")

# --- Arrow Keys ---
arrow_up_x = nav_cluster_start_x + w_1u + s
arrow_bottom_y = top_bottom_row
arrow_up_y = arrow_bottom_y - h_1u - s # Place Up arrow one row above the others

add_key("Up", arrow_up_x, arrow_up_y, w_1u, h_1u, "up")
add_key("Left", nav_cluster_start_x, arrow_bottom_y, w_1u, h_1u, "left")
add_key("Down", arrow_up_x, arrow_bottom_y, w_1u, h_1u, "down")
add_key("Right", arrow_up_x + w_1u + s, arrow_bottom_y, w_1u, h_1u, "right")

# Serialize to TOML
toml_data = {"key": keys}
with open("keys.toml", "w") as f:
    toml.dump(toml_data, f)

print("keys.toml generated successfully.")
