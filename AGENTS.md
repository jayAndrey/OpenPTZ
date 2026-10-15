# AGENTS.md

## Repository Overview

**OpenPTZ** is an open-source, high-performance PTZ (Pan-Tilt-Zoom) camera control suite written in Rust (2021 edition). It exposes four interfaces over a unified hardware abstraction layer:
- **Web GUI & REST API** (`src/api/`): Embedded `tiny_http` server with live HTML5/WebRTC video preview and JSON endpoints.
- **Terminal UI** (`src/tui/`): Full-screen interactive console interface using `ratatui` and `crossterm`.
- **CLI** (`src/cli.rs`): Scriptable non-interactive command-line interface via `clap` v4 with legacy `xMRi/PTZControl` compatibility flags.
- **Preset Manager** (`src/presets.rs`): Persistent coordinate and sensor storage in `~/.config/openptz/presets.json` with hardware EEPROM slot bridging.

The core Linux driver (`src/v4l2/`) operates directly on `/dev/video*` character devices through `libc::ioctl` calls, controlling standard V4L2 controls and USB UVC Extension Units (XU).

---

## Core Conventions

- **Rust Version**: 1.80+ (2021 edition).
- **Toolchain Environment**: When invoking cargo via bash, ensure `~/.cargo/bin` is in `PATH` (`export PATH="$HOME/.cargo/bin:$PATH"`).
- **Code Style & Formatting**: Standard `rustfmt`. Enforced via `cargo fmt --all -- --check`.
- **Linting Standard**: Zero Clippy warnings across all targets. Enforced via `cargo clippy --all-targets -- -D warnings`.
- **Versioning Policy**: Strict [Semantic Versioning 2.0.0](https://semver.org/). Breaking CLI/API changes bump MAJOR; new camera profiles or platform drivers bump MINOR; bugfixes bump PATCH.
- **Licensing**: Dual-licensed under `MIT OR Apache-2.0`. Preserve licensing headers and documentation integrity. Do not introduce dependencies incompatible with this dual license.
- **Safety**: Safe Rust is required for application logic. Low-level ioctl calls in `src/v4l2/ioctl.rs` encapsulate `unsafe` blocks behind safe Rust interfaces with bounds checking.
- **Documentation Policy**: Documentation in `docs/` (`ARCHITECTURE.md`, `docs/cameras/*.md`) must strictly contain high-level architectural information, hardware specifications, register maps, protocol codes (XU GUIDs, Unit IDs, Selectors), pins, and timing constraints. **Do not embed programming source code in documentation files.** Source code belongs strictly in `src/`.

---

## Architecture & Directory Structure

```
.
├── src/
│   ├── main.rs          # CLI entry point, command dispatch, and signal handling
│   ├── cli.rs           # Clap parser, subcommands (pan, tilt, zoom, preset, etc.), legacy flags
│   ├── presets.rs       # PresetManager: coordinate persistence (~/.config/openptz/presets.json)
│   ├── v4l2/            # Linux V4L2 driver and ioctl wrappers
│   │   ├── constants.rs # V4L2 control IDs (CID), UVC request codes, XU unit IDs
│   │   ├── ioctl.rs     # FFI bindings: ioctl_call(), V4l2Capability, UvcXuControlQuery
│   │   ├── device.rs    # CameraDevice controller: queries, bounds-checked control setters
│   │   └── mod.rs       # Re-exports CameraDevice, CameraStatus
│   ├── cameras/         # Modular camera profile subsystem
│   │   ├── mod.rs       # CameraProfile trait, CameraRegistry, AxisLimits, ZoomLimits
│   │   ├── generic.rs   # GenericUvcProfile: standard UVC PTZ fallback
│   │   └── logitech_rally.rs # LogitechRallyProfile: 3600 units/deg, XU home & EEPROM presets
│   ├── api/             # HTTP server and web assets
│   │   ├── mod.rs       # ApiServer routing, CORS headers, JSON serialization
│   │   └── web/         # Single-page HTML5/JS/CSS web interface embedded via include_str!
│   └── tui/             # Interactive Ratatui interface
│       └── mod.rs       # Terminal event loop, D-pad canvas, live coordinate gauges
├── docs/
│   ├── ARCHITECTURE.md          # Architectural spine, subsystem interactions, profile design (high-level)
│   └── cameras/
│       └── logitech_rally.md    # Hardware register maps, XU unit/selector specs, and quirks (high-level)
├── Cargo.toml           # Package metadata, release profiles, deb packaging rules
└── CONTRIBUTING.md      # Developer guide and contribution workflow
```

---

## Context Pointers

Consult detailed documents when working on specific branches:
- **Architecture design & modular profile pattern**: Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before refactoring device abstractions, profile traits, or cross-platform backends.
- **Logitech Rally camera specifics**: Read [`docs/cameras/logitech_rally.md`](docs/cameras/logitech_rally.md) when touching XU controls, coordinate conversion ratios, 50Hz/60Hz flicker, or sleep-wake timing.

---

## Guide: Adding a New Camera Profile in `src/cameras/`

To add support for a new PTZ camera (e.g. Aver, PTZOptics, Obsbot):

### 1. Hardware Inspection
Determine the device USB Vendor ID (VID), Product ID (PID), and V4L2 control limits:
```bash
lsusb -v -d <VID>:<PID>
v4l2-ctl -d /dev/videoX --all
v4l2-ctl -d /dev/videoX --list-ctrls-menus
```

### 2. Implement the `CameraProfile` Trait
Create `src/cameras/<vendor_model>.rs` implementing `CameraProfile`:
- Define profile metadata: unique string ID and human-readable name.
- Define device matching criteria: check USB VID/PID tuple and V4L2 card name string.
- Define mechanical axis limits: minimum and maximum degrees, step size, and raw units per degree.
- Define zoom limits: minimum and maximum magnification multipliers, step size, and raw range.
- Implement bidirectional coordinate conversion methods with bounds clamping.
- Implement the `goto_home` routine and optional hardware EEPROM preset methods.

### 3. Register in `src/cameras/mod.rs`
Add the new profile struct to the discovery registry vector in `src/cameras/mod.rs` (ensure the fallback `GenericUvcProfile` remains last).

### 4. Add Unit Tests
Every profile must include unit tests validating:
- Coordinate conversion round-trips (`deg_to_raw(raw_to_deg(x)) == x`).
- Clamping at negative and positive mechanical boundaries.
- Zoom factor interpolation between minimum and maximum magnification.

---

## Local Verification Commands

Run all checks in order before completing work. All commands must succeed with exit code `0`:

```bash
# 1. Ensure cargo is available in PATH
export PATH="$HOME/.cargo/bin:$PATH"

# 2. Check code formatting
cargo fmt --all -- --check

# 3. Check linter rules with zero warnings allowed
cargo clippy --all-targets -- -D warnings

# 4. Run test suite
cargo test
```
