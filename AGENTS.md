# Agent Instructions

This document provides instructions for software agents working with the `wayland-kbd-osd` repository.

## Development Environment Setup

**IMPORTANT:** Start by running the `install-dev-deps.sh` script to ensure all system-level dependencies are installed for your distribution:

```bash
./install-dev-deps.sh
```
This script attempts to detect your Linux distribution and install dependencies using the appropriate package manager. If your distribution is not supported, it will list the required dependencies. If you update this script or pull changes that modify it, please re-run it.

The script handles most dependencies. However, **Rust itself must be installed separately using [rustup](https://rustup.rs/)**. The `install-dev-deps.sh` script may install `cargo` if it's a system package, but `rustup` is the recommended way to manage Rust versions.

After running the script and ensuring Rust is installed, your environment should be set up. The core dependencies managed by the script include:

*   **Sway:** A Wayland compositor. Used for running tests in a headless environment.
*   **libudev-dev** (or equivalent): Development files for libudev.
*   **libinput-dev** (or equivalent): Development files for libinput.
*   **libcairo2-dev** (or equivalent): Development files for Cairo.
*   **libfreetype6-dev** (or equivalent): Development files for FreeType.
*   **libfontconfig1-dev** (or equivalent): Development files for Fontconfig.
*   **grim:** Screenshot utility for Wayland.
*   **dbus-x11** (or equivalent): D-Bus utilities.
*   **cargo:** Rust's package manager (also installed via rustup).

The `install-dev-deps.sh` script contains the most up-to-date list for supported distributions. If you modify project dependencies, please update `install-dev-deps.sh` accordingly.

On Debian-based systems (like Ubuntu), if you were to install manually (though the script is preferred), the command would be:
```bash
sudo apt-get update
# Note: This list is also in install-dev-deps.sh
sudo apt-get install -y sway libudev-dev libinput-dev libcairo2-dev libfreetype6-dev libfontconfig1-dev grim dbus-x11 cargo
```

## Running Tests

The startup test (`tests::test_startup_no_crash`) requires a Wayland environment and permissions to access input devices. It will attempt to start `sway --headless` automatically. Ensure that `sway` is installed and accessible in your `PATH`.

For the application to access keyboard input devices (e.g., `/dev/input/event*`), the user running the application must be a member of the `input` group. This is often necessary for debugging the libinput event loop or for full application functionality.

**To add your user to the `input` group:**

1.  Run the following command:
    ```bash
    sudo usermod -aG input $(whoami)
    ```
2.  For the group change to take effect, you must either:
    *   Log out and log back in.
    *   Or, for the current terminal session, run `newgrp input`. This will start a new shell session with the updated group membership. Commands needing input device access should be run from this new shell. Alternatively, you can use `sg input -c "your_command_here"` to run a specific command with the `input` group privileges.

After ensuring group membership, the tests, including the startup crash detection test, can be run using:

```bash
cargo test
```

## Visual Inspection with `take_screenshot.sh`

A script named `take_screenshot.sh` is provided to help with visual inspection of `wayland-kbd-osd` running in a headless Sway environment. This is particularly useful for agents or automated systems that need to verify the visual output of the OSD.

**Purpose:**

*   Starts a headless Sway session.
*   Runs `wayland-kbd-osd` within this session.
*   Takes a screenshot using the `grim` tool.
*   Saves the screenshot as `screenshot.png` in the repository root.

**Dependencies:**

The script requires the following to be installed and accessible in your `PATH`:

*   `sway`: For the headless Wayland compositor.
*   `grim`: For taking screenshots in Wayland.
*   `dbus-run-session`: (Usually part of `dbus-x11`) For starting Sway correctly in some environments.
*   `wayland-kbd-osd`: The application itself (it should be compiled, e.g., via `cargo build --release`).

On Debian-based systems, these can be installed with:
```bash
sudo apt-get update
# Ensure all build dependencies from the "Development Environment Setup" section are also installed for wayland-kbd-osd
sudo apt-get install -y sway grim dbus-x11 cargo
```
Ensure `wayland-kbd-osd` is built (e.g., `cargo build --release`). The script expects the binary at `target/release/wayland-kbd-osd`.

**Usage:**

```bash
./take_screenshot.sh
```

After execution, `screenshot.png` will be created.

**Important:** If you make any changes to the visual appearance of the OSD (e.g., key layout, colors, fonts, window size), you **must** regenerate the `screenshot.png` by running `./take_screenshot.sh` and commit the updated screenshot along with your code changes. This ensures the screenshot accurately reflects the current state of the application.

**Note on Input Device Errors:**
In environments without direct access to input devices (e.g., many CI systems), `wayland-kbd-osd` may log errors about being unable to open `/dev/input/event*` files. This is expected. The script's primary purpose is to verify that the OSD *displays* correctly; functional input reading in such restricted environments is a separate concern. The OSD should ideally still render its UI even if it cannot access physical input devices.

## Debugging with `run_headless.sh`

A script named `run_headless.sh` is provided for running `wayland-kbd-osd` in a headless Sway environment with input devices enabled. This is useful for debugging the libinput event loop and other input-related functionalities.

**Purpose:**

*   Builds the `wayland-kbd-osd` application (release mode).
*   Starts a headless Sway session.
*   Runs `wayland-kbd-osd` within this session, with input device access.
*   By default, keeps the application and Sway running for 10 seconds. This timeout can be configured.
*   Logs application output to `/tmp/app-headless.XXXXXX.log` and Sway output to `/tmp/sway-headless.XXXXXX.log`.

**Timeout Configuration:**

The script has an automatic shutdown mechanism to prevent it from running indefinitely.

*   **Default Timeout:** 10 seconds.
*   **Override Timeout:** Use the `-t <seconds>` flag to specify a different timeout duration. For example, `./run_headless.sh -t 30` will run for 30 seconds.
*   **Run Indefinitely:** Use `-t 0` to disable the timeout and run until manually terminated (Ctrl+C) or until the application exits on its own.

**Dependencies:**

The script requires the following to be installed and accessible in your `PATH`:

*   `sway`: For the headless Wayland compositor.
*   `dbus-run-session`: (Usually part of `dbus-x11`) For starting Sway correctly.
*   `cargo`: For building the application.
*   Rust development environment and other build dependencies for `wayland-kbd-osd` (see "Development Environment Setup").

On Debian-based systems, these can typically be installed with:
```bash
sudo apt-get update
# Ensure all build dependencies from the "Development Environment Setup" section are also installed
sudo apt-get install -y sway dbus-x11 cargo
```

**Usage:**

1.  Ensure your user is part of the `input` group and that the group membership is active for your current session (see "Running Tests" section for instructions on adding user to `input` group).
2.  Execute the script:
    ```bash
    ./run_headless.sh
    ```
    If you haven't started a new session or used `newgrp input` after adding your user to the `input` group, you might need to run it like this:
    ```bash
    sg input -c "./run_headless.sh"
    ```
    Example with a 5 second timeout:
    ```bash
    ./run_headless.sh -t 5
    ```
    Example to run indefinitely:
    ```bash
    ./run_headless.sh -t 0
    ```
3.  The script will build the application and then run it. Logs will be printed to temporary files (paths shown in script output).
4.  If a timeout is set (default or via `-t`), the script will terminate automatically after that duration. Otherwise (with `-t 0`), press `Ctrl+C` to terminate the script, which will also stop `wayland-kbd-osd` and `sway`.

This script is intended for active debugging. Check the application logs for information about input event processing.