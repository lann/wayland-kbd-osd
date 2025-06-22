# Agent Instructions

This document provides instructions for software agents working with the `wayland-kbd-osd` repository.

## Development Environment Setup

To build and test this project, you'll need the following dependencies:

*   **Rust:** Install Rust using [rustup](https://rustup.rs/).
*   **Weston:** A Wayland compositor. Used for running tests in a headless environment.
*   **libudev-dev:** Development files for libudev.
*   **libinput-dev:** Development files for libinput.

On Debian-based systems (like Ubuntu), you can install the non-Rust dependencies with:

```bash
sudo apt-get update
sudo apt-get install -y weston libudev-dev libinput-dev
```

## Running Tests

The startup test (`tests::test_startup_no_crash`) requires a Wayland environment and permissions to access . It will attempt to start `weston --headless` automatically. Ensure that `weston` is installed and accessible in your `PATH`.

The tests, including the startup crash detection test, can be run using:

```bash
cargo test
```

