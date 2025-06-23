#!/bin/bash

# --- Configuration ---
SWAY_LOG_FILE=$(mktemp /tmp/sway-headless.XXXXXX.log)
APP_LOG_FILE=$(mktemp /tmp/app-headless.XXXXXX.log)
APP_NAME="$(pwd)/target/release/wayland-kbd-osd"
DEFAULT_TIMEOUT=10 # Default timeout in seconds
current_timeout=$DEFAULT_TIMEOUT

# PIDs for cleanup
DBUS_SWAY_PID=""
APP_PID=""
SLEEP_PID=""

_cleanup_called=0
cleanup() {
    if [ "$_cleanup_called" -eq 1 ]; then return; fi
    _cleanup_called=1
    echo "CLEANUP: Starting cleanup..."

    if [ -n "$APP_PID" ] && ps -p "$APP_PID" > /dev/null; then
        echo "CLEANUP: Stopping application PID: $APP_PID..."
        kill -SIGTERM "$APP_PID" 2>/dev/null
        sleep 0.2
        kill -SIGKILL "$APP_PID" 2>/dev/null || true
    fi

    if [ -n "$SLEEP_PID" ] && ps -p "$SLEEP_PID" > /dev/null; then
        echo "CLEANUP: Stopping sleep PID: $SLEEP_PID..."
        kill -SIGKILL "$SLEEP_PID" 2>/dev/null || true
    fi

    if [ -n "$DBUS_SWAY_PID" ] && ps -p "$DBUS_SWAY_PID" > /dev/null; then
        echo "CLEANUP: Stopping dbus-run-session PID: $DBUS_SWAY_PID (and Sway)..."
        # Kill children of dbus-run-session (Sway) first
        pkill -P "$DBUS_SWAY_PID" 2>/dev/null
        sleep 0.2
        # Then kill dbus-run-session itself
        kill -SIGTERM "$DBUS_SWAY_PID" 2>/dev/null
        sleep 0.2
        kill -SIGKILL "$DBUS_SWAY_PID" 2>/dev/null || true
    fi

    # Fallback: try to kill by name if PIDs weren't captured or processes are still running
    # These are more likely to be needed if the script is killed externally before PIDs are set.
    pkill -f "$APP_NAME" 2>/dev/null
    pkill sway 2>/dev/null
    pkill dbus-run-session 2>/dev/null


    echo "CLEANUP: Sway log file: $SWAY_LOG_FILE"
    echo "CLEANUP: App log file: $APP_LOG_FILE"
    echo "CLEANUP: Cleanup finished."
}
trap cleanup EXIT SIGINT SIGTERM

# Parse command-line options
while getopts ":t:" opt; do
  case $opt in
    t)
      timeout_val="$OPTARG"
      if ! [[ "$timeout_val" =~ ^[0-9]+$ ]]; then
        echo "Error: Timeout value must be a non-negative integer." >&2
        exit 1
      fi
      current_timeout=$timeout_val
      ;;
    \?)
      echo "Invalid option: -$OPTARG" >&2
      exit 1
      ;;
    :)
      echo "Option -$OPTARG requires an argument." >&2
      exit 1
      ;;
  esac
done
shift $((OPTIND-1))

if [ "$current_timeout" -ne 0 ]; then
    echo "SCRIPT: Configured to run with a timeout of $current_timeout seconds."
else
    echo "SCRIPT: Configured to run indefinitely (timeout set to 0)."
fi

echo "SCRIPT: Building the application..."
if ! cargo build --release; then
    echo "SCRIPT: Failed to build the application. Exiting."
    exit 1
fi
if [ ! -f "$APP_NAME" ]; then
    echo "SCRIPT: Application binary not found at $APP_NAME after build. Exiting."
    exit 1
fi

echo "SCRIPT: Starting headless Sway with dbus-run-session..."
dbus-run-session env WLR_BACKENDS=headless sway --config /dev/null --verbose --unsupported-gpu &> "$SWAY_LOG_FILE" &
DBUS_SWAY_PID=$!
echo "SCRIPT: dbus-run-session PID: $DBUS_SWAY_PID. Waiting for Sway to initialize..."

WAYLAND_DISPLAY_NAME=""
for i in {1..15}; do
    if ! ps -p "$DBUS_SWAY_PID" > /dev/null; then
        echo "SCRIPT: Sway (dbus-run-session PID: $DBUS_SWAY_PID) exited prematurely. Logs:"
        cat "$SWAY_LOG_FILE"
        exit 1
    fi
    WAYLAND_DISPLAY_NAME=$(grep -oP "Running compositor on wayland display '\Kwayland-\d+(?='\s*$)" "$SWAY_LOG_FILE" || true)
    if [ -n "$WAYLAND_DISPLAY_NAME" ]; then
        echo "SCRIPT: Found WAYLAND_DISPLAY: $WAYLAND_DISPLAY_NAME"
        break
    fi
    sleep_duration=$(awk -v i="$i" 'BEGIN { print i * 0.1 + 0.5 }')
    sleep "$sleep_duration"
done

if [ -z "$WAYLAND_DISPLAY_NAME" ]; then
    echo "SCRIPT: Failed to find WAYLAND_DISPLAY in Sway's log ($SWAY_LOG_FILE) after waiting."
    cat "$SWAY_LOG_FILE"
    exit 1
fi

echo "SCRIPT: Starting $APP_NAME on WAYLAND_DISPLAY=$WAYLAND_DISPLAY_NAME..."
WAYLAND_DISPLAY="$WAYLAND_DISPLAY_NAME" "$APP_NAME" &> "$APP_LOG_FILE" &
APP_PID=$!
echo "SCRIPT: $APP_NAME started with PID $APP_PID."

if [ "$current_timeout" -gt 0 ]; then
    echo "SCRIPT: Waiting for app (PID $APP_PID) to exit or timeout of $current_timeout seconds to elapse."
    # Start sleep in the background
    sleep "$current_timeout" &
    SLEEP_PID=$!

    # Wait for either the application or the sleep to finish
    wait -n "$APP_PID" "$SLEEP_PID" 2>/dev/null
    APP_EXIT_CODE=$?

    # Check which process finished
    if ps -p "$APP_PID" > /dev/null; then
        # APP_PID is still running, so sleep must have finished (timeout)
        echo "SCRIPT: Timeout of $current_timeout seconds reached. Stopping application (PID $APP_PID)."
        kill -SIGTERM "$APP_PID" 2>/dev/null
        sleep 0.2
        kill -SIGKILL "$APP_PID" 2>/dev/null || true
        APP_EXIT_CODE=124 # Standard exit code for timeout
    else
        # Application exited before timeout
        echo "SCRIPT: Application (PID $APP_PID) exited with code $APP_EXIT_CODE before timeout."
    fi

    # Ensure sleep is killed if it's still running (e.g., if app exited very quickly)
    if [ -n "$SLEEP_PID" ] && ps -p "$SLEEP_PID" > /dev/null; then
        kill -SIGKILL "$SLEEP_PID" 2>/dev/null || true
    fi
    SLEEP_PID="" # Clear SLEEP_PID as it's handled
else
    echo "SCRIPT: Running until APP ($APP_PID) exits or script is terminated externally (no timeout)."
    wait "$APP_PID"
    APP_EXIT_CODE=$?
    echo "SCRIPT: Application (PID $APP_PID) exited with code $APP_EXIT_CODE. Script will now exit."
fi

# Cleanup will be called by EXIT trap.
exit $APP_EXIT_CODE
