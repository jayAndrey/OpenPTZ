# Contributing to OpenPTZ 🤝

Thank you for your interest in contributing to **OpenPTZ**! We welcome contributions from developers of all backgrounds, whether you're fixing bugs, adding new camera features, improving documentation, or adding support for new operating systems.

---

## 🗺️ Priority Roadmap: Cross-Platform Support

Our primary architectural goal is making OpenPTZ truly cross-platform:

1. **Linux Backend** (`src/v4l2/`) — **Complete ✅**
   - Video4Linux2 ioctl driver for Pan, Tilt, Zoom, Anti-flicker, Brightness, Auto-exposure.
2. **Windows Backend** (`src/platform/windows.rs`) — **Help Wanted 🙋‍♂️**
   - Implement native DirectShow `IAMCameraControl` or Windows Media Foundation `IMFExtendedCameraController` using the official [`windows`](https://crates.io/crates/windows) Rust crate.
3. **macOS Backend** (`src/platform/macos.rs`) — **Help Wanted 🙋‍♂️**
   - Implement UVC Camera Terminal control using CoreMediaIO DAL plugins or IOKit USB class control requests.

If you have experience with Windows Media Foundation or macOS CoreMediaIO/IOKit, opening a PR for these modules would be a massive contribution to the community!

---

## 🛠️ Development Setup

### 1. Prerequisites
- [Rust toolchain](https://rustup.rs/) (1.80 or newer):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- C compiler/linker:
  - Linux: `build-essential` or `clang` / `zig`
  - Windows: MSVC or GNU toolchain
  - macOS: Xcode Command Line Tools (`xcode-select --install`)

### 2. Clone and Build
```bash
git clone https://github.com/andryjay/OpenPTZ.git
cd OpenPTZ

# Build in debug mode
cargo build

# Run unit tests
cargo test

# Run interactive TUI
cargo run -- tui

# Run Web GUI
cargo run -- ui --no-browser
```

### 3. Testing Without a Physical PTZ Camera (Linux)
You can test OpenPTZ even without physical hardware using `v4l2loopback`:
```bash
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="Virtual PTZ Camera"
```

---

## 📋 Pull Request Guidelines

1. **Fork the Repository**: Create your feature branch (`git checkout -b feature/amazing-feature`).
2. **Follow Rust Conventions**:
   - Format your code:
     ```bash
     cargo fmt --all
     ```
   - Check with Clippy:
     ```bash
     cargo clippy --all-targets -- -D warnings
     ```
   - Ensure all tests pass:
     ```bash
     cargo test
     ```
3. **Commit Messages**: Write clear, descriptive commit messages.
4. **Submit a Pull Request**: Open a PR against the `master` branch of the main repository.

## 🏷️ Versioning Policy

OpenPTZ strictly adheres to [Semantic Versioning 2.0.0 (SemVer)](https://semver.org/):
- **MAJOR** version bumps (`x.0.0`) indicate breaking changes to CLI commands, REST API, or configuration formats.
- **MINOR** version bumps (`0.x.0`) add new functionality in a backwards-compatible manner (e.g. adding Windows or macOS platform drivers).
- **PATCH** version bumps (`0.0.x`) include backwards-compatible bug fixes and stability improvements.

---

## 📄 License and Copyright

By contributing to OpenPTZ, you agree that your contributions will be dual-licensed under both the **MIT License** and the **Apache License, Version 2.0**.
