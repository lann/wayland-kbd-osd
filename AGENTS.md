# Agent Instructions

This document provides instructions for software agents working with the `wayland-kbd-osd` repository.

## Development Environment Setup

**IMPORTANT:** Start by running the `install-dev-deps.sh` script to ensure all system-level dependencies are installed for your distribution:

```bash
./install-dev-deps.sh
```

This script attempts to detect your Linux distribution and install dependencies using the appropriate package manager. If your distribution is not supported, it will list the required dependencies. If you update this script or pull changes that modify it, please re-run it.

The script handles most dependencies. However, **Rust itself must be installed separately using [rustup](https://rustup.rs/)**. The `install-dev-deps.sh` script may install `cargo` if it's a system package, but `rustup` is the recommended way to manage Rust versions.

The `install-dev-deps.sh` script should contain all requirements needed to build and test this software, including supporting scripts. If you modify project dependencies, update `install-dev-deps.sh` accordingly.

**Note on Input Device Errors:**
In environments without direct access to input devices (e.g., many CI systems), `wayland-kbd-osd` may log errors about being unable to open `/dev/input/event*` files. This is expected. Recent improvements to logging (e.g., in `src/event.rs`) may provide more specific details from `libinput` if permission issues are the cause (look for messages like "Permission denied when opening /dev/input/..."). You can generally ignore these errors unless your task relates specifically to input device handling, in which case you will need to make these files accessible by e.g. adding the current user to the 'input' group and running `newgrp input`.

## Running Tests

**IMPORTANT:** `cargo test` MUST be run to validate any code changes before submitting them. All tests should pass.

The `install-dev-deps.sh` script above should install any dependencies needed to run tests.

The startup test (`tests::test_startup_no_crash`) requires a Wayland environment; it will attempt to start `sway --headless` automatically.

```bash
cargo test
```

## Code Style Guide

- Refactor proactively to improve the readability and maintainability of the code.
- Write code with Rust idioms like RAII where appropriate.
- Include doc comments for all `pub` functions, methods, structs, enums, and non-trivial type definitions. Consider adding comments for complex private logic as well.
- Add tests proactively, especially for complex logic.
- Any comments containing commentary about a change should be removed before committing.
- If code is commented out while making changes, remove the commented out code before committing.


## Visual Inspection with `take_screenshot.sh`

**Important:** If you make any changes to the visual appearance of the OSD (e.g., key layout, colors, fonts, window size), you **must** regenerate the `screenshot.png` by running `./take_screenshot.sh` and commit the updated screenshot along with your code changes. This ensures the screenshot accurately reflects the current state of the application.

A script named `take_screenshot.sh` is provided to help with visual inspection of `wayland-kbd-osd` running in a headless Sway environment. This is particularly useful for agents or automated systems that need to verify the visual output of the OSD.

The `install-dev-deps.sh` script above should install any dependencies needed to run this script.

```bash
./take_screenshot.sh
```

After execution, `screenshot.png` will be created.

## Debugging with `run_headless.sh`

A script named `run_headless.sh` is provided for running `wayland-kbd-osd` in a headless Sway environment. This is useful for debugging the libinput event loop and other input-related functionalities.

The `install-dev-deps.sh` script above should install any dependencies needed to run this script.

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
