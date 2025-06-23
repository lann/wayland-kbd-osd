#!/bin/bash

# Script to install development dependencies for wayland-kbd-osd
# This script should be updated if dependencies change.

# Exit immediately if a command exits with a non-zero status.
set -e

# --- Define Dependencies ---

# Common dependencies (names might vary slightly per distro)
# sway: Wayland compositor (for tests and headless runs)
# libudev-dev: Development files for libudev
# libinput-dev: Development files for libinput
# libcairo2-dev: Development files for Cairo graphics library
# libfreetype6-dev: Development files for FreeType font rendering
# libfontconfig1-dev: Development files for Fontconfig library
# grim: Screenshot utility for Wayland
# dbus-x11: D-Bus utilities (often needed for Sway)
# cargo: Rust's package manager and build tool (may be part of rust installation)

DEBIAN_DEPS=(
    sway
    libudev-dev
    libinput-dev
    libcairo2-dev
    libfreetype6-dev
    libfontconfig1-dev
    grim
    dbus-x11
    cargo  # cargo might also come from rustup, but good to include if available
)

ARCH_DEPS=(
    sway
    systemd-libs # Provides libudev on Arch
    libinput
    cairo
    freetype2
    fontconfig
    grim
    dbus
    rust # Includes cargo on Arch
)

# --- Helper Functions ---

echo_info() {
    echo "[INFO] $1"
}

echo_warning() {
    echo "[WARNING] $1"
}

echo_error() {
    echo "[ERROR] $1" >&2
}

# --- Distribution Detection and Installation ---

if [ -f /etc/os-release ]; then
    # freedesktop.org and systemd
    . /etc/os-release
    OS_ID=$ID
elif type lsb_release >/dev/null 2>&1; then
    # linuxbase.org
    OS_ID=$(lsb_release -si | tr '[:upper:]' '[:lower:]')
elif [ -f /etc/lsb-release ]; then
    # For some versions of Debian/Ubuntu without lsb_release command
    . /etc/lsb-release
    OS_ID=$DISTRIB_ID | tr '[:upper:]' '[:lower:]'
elif [ -f /etc/debian_version ]; then
    # Older Debian/Ubuntu/etc.
    OS_ID="debian"
elif [ -f /etc/arch-release ]; then
    OS_ID="arch"
else
    # Fallback to uname, e.g. "Linux <version>", "Darwin <version>", ...
    OS_ID=$(uname -s | tr '[:upper:]' '[:lower:]')
fi

echo_info "Detected OS ID: $OS_ID"

install_rust_reminder() {
    echo_info "---------------------------------------------------------------------"
    echo_info "This script does not install Rust itself."
    echo_info "Please ensure Rust and cargo are installed, preferably via rustup:"
    echo_info "  https://rustup.rs/"
    echo_info "If 'cargo' was installed as a system package, it might be sufficient."
    echo_info "---------------------------------------------------------------------"
}

if [[ "$OS_ID" == "debian" || "$OS_ID" == "ubuntu" || "$OS_ID" == "linuxmint" || "$OS_ID" == "pop" ]]; then
    echo_info "Detected Debian-based distribution."
    echo_info "Attempting to install packages using apt..."
    echo_info "Required packages: ${DEBIAN_DEPS[*]}"
    sudo apt-get update
    sudo apt-get install -y "${DEBIAN_DEPS[@]}"
    install_rust_reminder
    echo_info "Debian/Ubuntu dependencies installation complete."
elif [[ "$OS_ID" == "arch" || "$OS_ID" == "manjaro" || "$OS_ID" == "endeavouros" ]]; then
    echo_info "Detected Arch-based distribution."
    echo_info "Attempting to install packages using pacman..."
    echo_info "Required packages: ${ARCH_DEPS[*]}"
    sudo pacman -Syu --noconfirm "${ARCH_DEPS[@]}"
    # Rust is installed as part of 'rust' package which includes cargo.
    # No separate rustup reminder needed if 'rust' package is successfully installed.
    echo_info "Arch Linux dependencies installation complete."
else
    echo_warning "Unsupported or unknown distribution: $OS_ID"
    echo_info "Please install the following dependencies manually:"
    echo_info "  - sway (Wayland compositor)"
    echo_info "  - libudev (development files)"
    echo_info "  - libinput (development files)"
    echo_info "  - cairo (graphics library, development files)"
    echo_info "  - freetype (font rendering, development files)"
    echo_info "  - fontconfig (font management, development files)"
    echo_info "  - grim (Wayland screenshot utility)"
    echo_info "  - dbus (D-Bus utilities)"
    echo_info "  - Rust and cargo (build system and package manager for Rust)"
    echo_info "Package names may vary. Refer to your distribution's documentation."
    install_rust_reminder
fi

echo_info "Dependency check/installation script finished."
echo_info "Remember to add your user to the 'input' group if you haven't already:"
echo_info "  sudo usermod -aG input \$(whoami)"
echo_info "And then log out and log back in, or run 'newgrp input' in your current terminal."
