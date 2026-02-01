# GPU Monitor

A high-performance, modern GPU monitoring tool for Linux, featuring both a beautiful Desktop GUI and a feature-rich Terminal UI (CLI). Built with Rust and Tauri.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Linux-green.svg)

## Features

### 🖥️ Desktop GUI
- **Modern Design**: macOS-inspired minimalist interface with dark mode support.
- **Adaptive Layout**:
  - **Single GPU**: Expanded view with detailed waveforms and full process list.
  - **Multi-GPU**: Compact grid view with quick status summaries.
- **Real-time Charts**: Smooth, hardware-accelerated visualization of GPU load and memory usage.
- **Process Management**: View active processes, filter by name/PID, and see memory usage.

### 📟 Terminal UI (CLI)
- **Interactive Dashboard**: Full TUI with real-time sparkline charts (`--watch` mode).
- **Lightweight**: Minimal resource footprint, perfect for servers or SSH sessions.
- **Scriptable**: JSON output support for integration with other tools.

## Requirements

- **OS**: Linux (Tested on Ubuntu 22.04/24.04)
- **Hardware**: NVIDIA GPU(s)
- **Drivers**: Proprietary NVIDIA drivers installed (libnvidia-ml)

### Build Dependencies (Ubuntu/Debian)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## Installation

### From Source

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/gpu-monitor.git
   cd gpu-monitor
   ```

2. **Build & Install CLI**
   ```bash
   cargo install --path crates/gpu-monitor-cli
   ```

3. **Build & Install GUI**
   ```bash
   cd crates/gpu-monitor-gui
   # Install frontend dependencies
   cd src-web && npm install && cd ..
   # Build .deb package
   cargo tauri build
   # Install
   sudo dpkg -i target/release/bundle/deb/gpu-monitor_*.deb
   ```

## Usage

### CLI Mode

```bash
# Launch interactive TUI dashboard (Recommended)
gpu-monitor --watch

# Single snapshot (like nvidia-smi)
gpu-monitor --once

# JSON output for scripts
gpu-monitor --json

# Show processes only
gpu-monitor processes
```

### GUI Mode

Launch from your application menu or run:
```bash
gpu-monitor-gui
```

## Project Structure

- `crates/gpu-monitor-core`: Shared library for NVML bindings and data models.
- `crates/gpu-monitor-cli`: Terminal-based monitoring tool (Ratatui).
- `crates/gpu-monitor-gui`: Desktop application (Tauri v2 + React).

## License

MIT License
