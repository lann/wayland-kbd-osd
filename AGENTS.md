# Agent Instructions

This document provides instructions for software agents working with the `wayland-kbd-osd` repository.

## Development Environment Setup

To build and test this project, you'll need the following dependencies:

*   **Rust:** Install Rust using [rustup](https://rustup.rs/).
*   **Sway:** A Wayland compositor. Used for running tests in a headless environment.
*   **libudev-dev:** Development files for libudev.
*   **libinput-dev:** Development files for libinput.

On Debian-based systems (like Ubuntu), you can install the non-Rust dependencies with:

```bash
sudo apt-get update
sudo apt-get install -y sway libudev-dev libinput-dev
```

## Running Tests

The startup test (`tests::test_startup_no_crash`) requires a Wayland environment and permissions to access . It will attempt to start `sway --headless` automatically. Ensure that `sway` is installed and accessible in your `PATH`.

The tests, including the startup crash detection test, can be run using:

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
sudo apt-get install -y sway grim dbus-x11 cargo # Add other build deps for wayland-kbd-osd if not already listed
```
Ensure `wayland-kbd-osd` is built (e.g., `cargo build --release`). The script expects the binary at `target/release/wayland-kbd-osd`.

**Usage:**

```bash
./take_screenshot.sh
```

After execution, `screenshot.png` will be created.

**Note on Input Device Errors:**
In environments without direct access to input devices (e.g., many CI systems), `wayland-kbd-osd` may log errors about being unable to open `/dev/input/event*` files. This is expected. The script's primary purpose is to verify that the OSD *displays* correctly; functional input reading in such restricted environments is a separate concern. The OSD should ideally still render its UI even if it cannot access physical input devices.