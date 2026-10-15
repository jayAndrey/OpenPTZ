# OpenPTZ 🎥✨

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![SemVer 2.0.0](https://img.shields.io/badge/SemVer-2.0.0-blue.svg)](https://semver.org/)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg?logo=rust)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey.svg)](#-cross-platform-architecture--roadmap)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

**OpenPTZ** is a modern, high-performance, open-source PTZ (Pan-Tilt-Zoom) camera control suite written in pure **Rust**. Designed for broadcast studios, conference rooms, streamers, and automation engineers, it provides an ultra-responsive Web/Desktop GUI with live video preview, an interactive terminal UI (TUI), a scriptable CLI, and a local REST API.

Originally created to solve the notorious lack of official Linux control and 50Hz/60Hz anti-flicker tools for the **Logitech Rally Camera**, OpenPTZ is architected as an extensible, cross-platform solution for all UVC PTZ cameras.

---

## 🌟 Highlights

* **🎯 Live Preview**: Native camera capture in the desktop window and an MJPEG stream from the OpenPTZ host for network clients.
* **⚡ Native Desktop Window**: The `gui` command runs without a browser or local HTTP server.
* **💾 Rock-Solid Coordinate Presets**: Fast 1–8 numbered slots and unlimited named presets (`podium`, `whiteboard`, `audience`) saved in `presets.json` with Pan, Tilt, Zoom, brightness, and anti-flicker state.
* **💡 Anti-Flicker Frequency Control**: Instant switching between `50 Hz` (PAL / Europe) and `60 Hz` (NTSC / Americas) to eliminate artificial lighting banding.
* **🕹️ Multiple Control Interfaces**:
  * **Native GUI**: Local live video, D-Pad, zoom, Home, and preset controls.
  * **Network UI**: Browser controls, live video from the OpenPTZ host, and REST endpoints.
  * **Terminal UI (TUI)**: Interactive Ratatui console interface for SSH sessions and headless servers.
  * **Command Line (CLI)**: Fast, non-interactive commands for scripting, cron jobs, and shortcuts.
  * **HTTP REST API**: Network control for **Bitfocus Companion**, **Elgato Stream Deck**, **OBS Studio**, and Home Assistant.
* **🔄 Backward Compatibility**: Supports legacy CLI flags from Windows `xMRi/PTZControl`.

---

## 🖥️ User Interfaces

### 1. Native GUI (`openptz gui`)

Open a local application window with camera preview and PTZ controls:
```bash
openptz gui
```
This mode does not launch a browser or listen on an HTTP port. Camera access is still subject to operating system permissions.

Features: camera and resolution selection, live preview with PTZ controls and presets, snapshots, click-to-aim, and automatic video reconnect after the camera is unplugged and reattached.

### 2. Network UI (`openptz ui`)

```bash
openptz ui --port 8080 --no-browser
```

Open `http://localhost:8080` on the host or `http://HOST_IP:8080` on another device on the network. The page displays video captured on the **OpenPTZ host**, so the client browser does not need camera permission. PTZ controls operate that same host camera. Without `--no-browser`, OpenPTZ also opens the local page automatically. The server listens on all network interfaces; use it only on trusted networks.

The live stream is also available at `/api/video.mjpg`, with capture status at `/api/video/status`.

---

### 3. Interactive Terminal UI (`openptz tui`)

Run a full-screen interactive console interface inside any terminal:
```bash
openptz tui
# or simply:
openptz
```

**Keybindings:**
* `←` / `→` — Pan left / right
* `↑` / `↓` — Tilt up / down
* `+` / `-` or `PgUp` / `PgDn` — Zoom in / out
* `Home` / `h` — Return to home position (`0.0°`, `0.0°`, `1.0x`)
* `1` .. `8` — Recall Preset 1–8
* `Shift + 1..8` — Save current position to Preset 1–8
* `F` — Toggle Anti-Flicker (`Off` → `50 Hz` → `60 Hz`)
* `[` / `]` — Change step size (1°, 5°, 10°, 20°)
* `B` / `b` — Adjust Brightness
* `C` / `c` — Adjust Contrast
* `q` / `Esc` — Quit

---

### 3. Command Line Interface (CLI)

Fast, scriptable commands for automation:

```bash
# Query camera status
openptz status
openptz status --json

# List available cameras
openptz list

# Pan, Tilt & Zoom commands
openptz pan --to 25.5               # Move Pan to +25.5°
openptz pan --left --step 5.0        # Step Pan Left by 5°
openptz tilt --to -10.0              # Move Tilt to -10.0°
openptz zoom --to 3.5                # Set Zoom to 3.5x
openptz home                         # Reset to home position

# Anti-Flicker (50Hz / 60Hz)
openptz set --flicker 50             # 50 Hz (Europe / PAL)
openptz set --flicker 60             # 60 Hz (Americas / NTSC)

# Presets
openptz preset --save 1              # Save current position to Slot 1
openptz preset --recall 1            # Recall Slot 1
openptz preset --save-soft podium    # Save named preset 'podium'
openptz preset --recall-soft podium  # Recall named preset 'podium'
openptz preset                       # List all saved presets
```

---

### 4. HTTP REST API

OpenPTZ embeds a lightweight HTTP server for network automation:

| Endpoint | Method | Description |
| :--- | :--- | :--- |
| `/api/status` | `GET` | Returns full JSON camera status (Pan, Tilt, Zoom, settings) |
| `/api/home` | `POST` | Resets camera to home position |
| `/api/pan?deg=15.0` | `POST` | Sets absolute Pan angle |
| `/api/pan?dir=left&step=5.0` | `POST` | Steps Pan relative |
| `/api/tilt?deg=-5.0` | `POST` | Sets absolute Tilt angle |
| `/api/tilt?dir=up&step=5.0` | `POST` | Steps Tilt relative |
| `/api/zoom?x=2.5` | `POST` | Sets Zoom multiplier (1.0 to 15.0) |
| `/api/zoom?dir=in&step=0.5` | `POST` | Steps Zoom relative |
| `/api/flicker?hz=50` | `POST` | Sets anti-flicker (0: Off, 50: 50Hz, 60: 60Hz) |
| `/api/preset/list` | `GET` | Returns list of all presets (slots & named) |
| `/api/preset/save/<slot>` | `POST` | Saves current view into slot (1–8) |
| `/api/preset/recall/<slot>` | `POST` | Moves camera to preset slot (1–8) |
| `/api/preset/save?name=podium` | `POST` | Saves named preset |
| `/api/preset/recall?name=podium` | `POST` | Recalls named preset |
| `/api/preset/delete?name=podium` | `POST` | Deletes named preset |

#### Bitfocus Companion / Stream Deck Integration
Create a generic HTTP Request button in Companion:
- **URL**: `http://127.0.0.1:8080/api/preset/recall/1`
- **Method**: `POST`

---

---

## 🏗️ Cross-Platform Architecture

OpenPTZ separates camera control into platform-agnostic application logic (GUI, TUI, CLI, REST API, Presets) and native OS hardware drivers:

```
OpenPTZ Architecture
├── Native GUI (egui)
├── Network UI (HTML5 + MJPEG / Vanilla JS)
├── TUI (Ratatui / Crossterm)
├── CLI (Clap v4)
├── REST API (tiny_http)
└── Camera Hardware Abstraction Layer
    ├── Linux: Video4Linux2 (V4L2 ioctl + Logitech XU) ✅
    ├── Windows: Native DirectShow (IAMCameraControl + IAMVideoProcAmp) ✅
    └── macOS: AVFoundation capture + external uvc-util PTZ (errors reported)
```

| Platform | Native Backend Driver | Features | Verification status |
| :--- | :--- | :--- | :--- |
| **Windows** | Media Foundation video; DirectShow PTZ (`IAMCameraControl` & `IAMVideoProcAmp`) | Native GUI, web UI video, Pan/Tilt/Zoom, presets | PTZ and video **verified on Logitech Rally hardware** ✅ |
| **Linux** | Video4Linux2 (`v4l2` ioctls) | Pan/Tilt/Zoom, Logitech XU presets, 50/60Hz anti-flicker | Builds in CI; **hardware verification pending** |
| **macOS** | AVFoundation capture; PTZ via external `uvc-util` | Native GUI and web UI preview; PTZ commands report errors | Builds in CI; **PTZ unverified on hardware** |

---

## 📦 Installation

### Linux

#### Option 1: AppImage (Zero Installation, Recommended) 🚀
Download `OpenPTZ-x86_64.AppImage` from [GitHub Releases](https://github.com/jayAndrey/OpenPTZ/releases):
```bash
chmod +x OpenPTZ-x86_64.AppImage
./OpenPTZ-x86_64.AppImage
```

#### Option 2: Ubuntu / Debian Package (`.deb`)
Download `openptz-linux-amd64.deb` from [GitHub Releases](https://github.com/jayAndrey/OpenPTZ/releases):
```bash
sudo dpkg -i openptz-linux-amd64.deb
```

#### Option 3: Portable Tarball
Download `openptz-linux-x86_64.tar.gz` from [GitHub Releases](https://github.com/jayAndrey/OpenPTZ/releases):
```bash
tar -xzf openptz-linux-x86_64.tar.gz
cp openptz/openptz ~/.local/bin/
```

### Windows

Download `openptz-windows-x86_64.zip` from [GitHub Releases](https://github.com/jayAndrey/OpenPTZ/releases):
1. Extract the `.zip` archive.
2. Run `openptz.exe gui` in PowerShell or Command Prompt for the native window.
3. Automatically interfaces with DirectShow cameras (Logitech Rally, PTZ Pro, and all UVC webcams).

### macOS (Apple Silicon & Intel)

Download `openptz-macos-aarch64.tar.gz` (Apple Silicon M1/M2/M3/M4) or `openptz-macos-x86_64.tar.gz` (Intel) from [GitHub Releases](https://github.com/jayAndrey/OpenPTZ/releases):
```bash
tar -xzf openptz-macos-aarch64.tar.gz
sudo cp openptz/openptz /usr/local/bin/
openptz gui
```

### Build from Source (All Platforms)

**Requirements**: Rust 1.88+ (`cargo`).

```bash
# Clone repository
git clone https://github.com/jayAndrey/OpenPTZ.git
cd OpenPTZ

# Build optimized binary
cargo build --release

# The compiled binary is at:
# Linux / macOS: target/release/openptz
# Windows: target/release/openptz.exe
```

Verify installation:
```bash
openptz --version
openptz list
```

---

## 🔧 Hardware Notes: Logitech Rally Camera

Logitech Rally cameras (`046d:0881`) on Linux require careful UVC handling:
1. **Extension Units (XU)**: Rally's Selector 2 (`PANTILT_RESET`) only accepts values `0..3` (`3` = Hardware Home). Sending values > 3 causes internal USB endpoint stall (`Device or resource busy`). OpenPTZ avoids invalid values and implements software coordinate presets.
2. **Anti-Flicker**: European/Asian fluorescent and LED lighting oscillates at 50Hz. Always set `--flicker 50` or click **50 Hz** in the UI to prevent horizontal banding lines.

---

## 🙏 Acknowledgements & Prior Art

OpenPTZ was inspired by and builds upon community research and prior work:
* [xMRi/PTZControl](https://github.com/xMRi/PTZControl) — Reference Windows C++ implementation exploring Logitech UVC Extension Unit descriptors.
* [Himanshu Chaudhary's Ubuntu Guide](https://medium.com/@himanshu.chaudhary) — Documenting V4L2 power line frequency control on Ubuntu Linux.

---

## 🤝 Contributing

Contributions make the open-source community an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

Please read our [Contributing Guidelines](CONTRIBUTING.md) for details on code of conduct and the development process.

---

## 📄 License

OpenPTZ is dual-licensed under either of:

* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* **MIT license** ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution Terms

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
