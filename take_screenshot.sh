#!/bin/bash

# Exit on error
set -e

# --- Configuration ---
SWAY_LOG_FILE=$(mktemp /tmp/sway-headless.XXXXXX.log)
SCREENSHOT_FILE="screenshot.png"
# Corrected APP_NAME to be the full path to the compiled binary
APP_NAME="$(pwd)/target/release/wayland-kbd-osd"

# --- Cleanup function ---
cleanup() {
    echo "Cleaning up..."
    # Kill Sway and any child processes
    if [ -n "$SWAY_PID" ]; then
        pkill -P "$SWAY_PID" # Kill children of Sway
        kill "$SWAY_PID" || true # Kill Sway itself
    fi
    # Kill the app if it's still running (e.g., if Sway failed to start)
    pkill -f "$APP_NAME" || true
    rm -f "$SWAY_LOG_FILE"
    echo "Cleanup finished."
}

# Trap exit signals to ensure cleanup
trap cleanup EXIT SIGINT SIGTERM

# --- Build the application ---
echo "Building the application in release mode..."
if ! cargo build --release; then
    echo "Failed to build the application. Exiting."
    exit 1
fi
echo "Build successful."

# --- Start Sway ---
echo "Starting headless Sway with dbus-run-session and specific WLR environment variables..."
# Added --unsupported-gpu, which is often needed for headless setups
# Launching sway within dbus-run-session
# Explicitly setting WLR_BACKENDS and WLR_LIBINPUT_NO_DEVICES for the sway command
dbus-run-session env WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway --config /dev/null --verbose --unsupported-gpu &> "$SWAY_LOG_FILE" &
SWAY_PID=$! # This will be the PID of dbus-run-session

echo "dbus-run-session PID: $SWAY_PID. Waiting for Sway to initialize and log WAYLAND_DISPLAY..."
# Note: The actual sway process will be a child of dbus-run-session.
# The cleanup function should ideally kill the process group or use pkill for sway.
# The current pkill -P $SWAY_PID should work if sway is a direct child.
# If sway is a grandchild, more sophisticated process group killing might be needed,
# but for now, we'll rely on pkill -f sway in the trap as a fallback.

# --- Extract WAYLAND_DISPLAY ---
WAYLAND_DISPLAY_NAME=""
# Wait for a few seconds for Sway to start and output the display name
for i in {1..15}; do # Try for up to 15 seconds (15 * 0.5s = 7.5s, then 15 * 1s for sway)
    # Check if Sway is still running
    if ! ps -p $SWAY_PID > /dev/null; then
        echo "Sway process $SWAY_PID exited prematurely. Logs:"
        cat "$SWAY_LOG_FILE"
        exit 1
    fi

    # Try to extract WAYLAND_DISPLAY from the log
    WAYLAND_DISPLAY_NAME=$(grep -oP "Running compositor on wayland display '\Kwayland-\d+(?='\s*$)" "$SWAY_LOG_FILE" || true)
    if [ -n "$WAYLAND_DISPLAY_NAME" ]; then
        echo "Found WAYLAND_DISPLAY: $WAYLAND_DISPLAY_NAME"
        break
    fi
    # Initial short sleeps, then longer ones if sway is still starting
    if [ $i -le 5 ]; then
        sleep 0.5
    else
        sleep 1
    fi
done

if [ -z "$WAYLAND_DISPLAY_NAME" ]; then
    echo "Failed to find WAYLAND_DISPLAY in Sway's log ($SWAY_LOG_FILE) after waiting."
    echo "Sway log content:"
    cat "$SWAY_LOG_FILE"
    exit 1
fi

# --- Run the application ---
echo "Starting $APP_NAME --overlay on WAYLAND_DISPLAY=$WAYLAND_DISPLAY_NAME..."
WAYLAND_DISPLAY="$WAYLAND_DISPLAY_NAME" "$APP_NAME" --overlay &
APP_PID=$!

# Wait for the application to start
echo "Waiting 1 second for $APP_NAME to initialize..."
sleep 1

# Check if app is still running
if ! ps -p $APP_PID > /dev/null; then
    echo "$APP_NAME process $APP_PID exited prematurely."
    echo "Sway log content:"
    cat "$SWAY_LOG_FILE"
    # Attempt to get app logs if possible - this part is highly dependent on how the app logs
    # For now, we'll just note it exited.
    exit 1
fi


# --- Take screenshot ---
echo "Taking screenshot with grim..."
if WAYLAND_DISPLAY="$WAYLAND_DISPLAY_NAME" grim "$SCREENSHOT_FILE"; then
    echo "Screenshot saved to $SCREENSHOT_FILE"
else
    echo "Failed to take screenshot."
    echo "Sway log content:"
    cat "$SWAY_LOG_FILE"
    exit 1
fi

echo "Script finished successfully."
# Cleanup will be called automatically on exit.
exit 0
