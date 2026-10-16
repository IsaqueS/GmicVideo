# G'MIC Video Effects Editor

<!-- Project Badges: Replace or add relevant badges -->
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/Python-3.10+-blue.svg)](https://www.python.org/)
[![Framework](https://img.shields.io/badge/UI-KDE%20Kirigami%20%7C%20PySide6-blueviolet.svg)](https://develop.kde.org/frameworks/kirigami/)
[![License](https://img.shields.io/badge/License-GPLv3-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux-lightgrey.svg)](https://www.kernel.org/)

A high-performance, non-linear video effects editor powered by **Rust**, **G'MIC**, **FFmpeg**, and the **KDE Kirigami Framework**. Designed as the spiritual successor to `ultrawarp-batch`, G'MIC Video combines the full image-processing power of over 1,100 G'MIC filters with parameter tweening, mathematical curve sculpting, and multi-threaded parallel rendering.

---

## 📸 Screenshots

<!-- FILL IN: Add your interface screenshots and GIFs below -->
| Main Effects Workspace | Inline Curve Editor |
| :---: | :---: |
| ![Effects Workspace](<!-- FILL IN: Path or URL to workspace screenshot -->) | ![Curve Editor](<!-- FILL IN: Path or URL to curve screenshot -->) |

| Independent Render Manager & Task Queue | Video Cut & Trimming |
| :---: | :---: |
| ![Render Queue](<!-- FILL IN: Path or URL to render queue screenshot -->) | ![Video Cut](<!-- FILL IN: Path or URL to cut slider screenshot -->) |

---

## ✨ Features

### 🎨 1,100+ G'MIC Visual Effects
- Real-time search and categorization across the entire G'MIC filter library:
  - Distortions, Halftones, Glitches, Film Emulation, Color Grading, Warps, Stylizations, and more.
- Dynamic property controls (Sliders, SpinBoxes, Color selectors, Switches) tailored to each effect's schema.

### 📈 Parameter Animation & Curve Sculpting
Every numeric filter parameter can be animated over the video duration via the **Animate** menu:
- **Disabled**: Fixed parameter value (default).
- **Random**: Generates deterministic pseudo-random values frame-by-frame within defined amplitude bounds.
- **Curve**: Interactive mathematical animation curve:
  - **28 Built-in Mathematical Easing Presets**:
    - Linear diagonal
    - Quad, Cubic, Quart, Sine, Expo, Circular
    - Back / Overshoot
    - Elastic (sinusoidal spring decay)
    - Bounce (realistic multi-arc ground bounce)
  - **Custom Editable Bézier Mode**: Real-time interactive canvas with draggable control handles ($P_1$ & $P_2$) for custom ease curves.
  - **Amplitude Range Slider**: Set minimum and maximum parameter boundary limits.

### 💾 Preset System (`.geffect`)
- Export and import complete effect configurations including active parameters, animation modes, and curve definitions.
- Saved in human-readable JSON format with percentage-normalized timeline definitions (`0.0` to `1.0`).
- Strict schema validation on file import.

### ✂️ Video Cut & Trimming
- Interactive dual-handle range slider to select start and end trimming times directly below the video timeline.
- Frame extraction and audio alignment automatically adjust to the selected cut range.

### 🚀 Independent Background Render Manager & Queue
- Runs in a separate window so you can continue editing new videos while previous renders process in the background.
- Multi-task queue with real-time status cards (Queued, Rendering, Completed, Failed).
- Simplified rendering settings:
  - **Quality Slider**: Draft to Maximum Quality (automatically mapped to optimal CRF values).
  - **Encoding Speed Slider**: Maximum Compression to Ultrafast Encode.
  - **Smooth Fading on Lossless**: Quality and speed sliders fade out gracefully when Lossless RGB is selected.

### 🎬 Multi-Format Video Export
- **H.264 (MP4)**: Standard compatibility (default).
- **H.265 / HEVC (MP4)**: High-efficiency modern compression.
- **VP9 (WebM)**: Open web video format.
- **AV1 (MP4)**: Next-generation ultra-compressed format via SVT-AV1.
- **Lossless RGB (MP4)**: Mathematically exact 1:1 pixel fidelity with zero compression artifacts.
- Automatic even-dimension padding (`pad=ceil(iw/2)*2:ceil(ih/2)*2`) ensures non-standard video resolutions (e.g. `971x356`) encode cleanly without FFmpeg errors.
- Real-time FFmpeg encoding progress bar with live frame counter, FPS speed, and remaining time estimation.

---

## 🛠️ Tech Stack & Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                 Frontend UI (PySide6 / QML)                 │
│  - KDE Kirigami Framework (Adaptive, responsive desktop UX) │
│  - Inline Curve Canvas & Real-time Timeline Scrubber        │
│  - Independent Render Window & Multi-Task Queue             │
└──────────────────────────────┬──────────────────────────────┘
                               │ JSON-over-stdio IPC
┌──────────────────────────────▼──────────────────────────────┐
│                  Backend Core (Rust 2021)                   │
│  - Rayon: Work-stealing multi-threaded frame rendering      │
│  - FFmpeg & FFprobe: High-speed extraction & audio muxing   │
│  - G'MIC CLI Engine: Image computing & mathematical filters │
└─────────────────────────────────────────────────────────────┘
```

---

## 📋 System Requirements

<!-- FILL IN: Update any minimum requirements -->
- **OS**: Linux (Fedora, Ubuntu/Debian, Arch Linux, openSUSE, etc.)
- **Rust**: `1.75+` (with `cargo`)
- **Python**: `3.10+` with `PySide6`
- **KDE Frameworks**: `kf6-kirigami` (or `kf5-kirigami`)
- **System Binaries**:
  - `ffmpeg` & `ffprobe`
  - `gmic` (`3.0+`)

---

## 🚀 Building & Running

### 1. Clone Repository
```bash
git clone https://github.com/<!-- FILL IN: username -->/GmicVideo.git
cd GmicVideo
```

### 2. Install Dependencies

#### Fedora / RHEL:
```bash
sudo dnf install cargo python3 python3-pip ffmpeg gmic kf6-kirigami
pip install PySide6
```

#### Ubuntu / Debian:
```bash
sudo apt install cargo python3 python3-pip ffmpeg gmic qml-module-org-kde-kirigami2
pip install PySide6
```

#### Arch Linux:
```bash
sudo pacman -S rust python python-pip ffmpeg gmic kirigami
pip install PySide6
```

### 3. Build Rust Backend
```bash
cargo build --release
```

### 4. Launch the Application
```bash
python3 src_gui/gmicvideo_gui.py
```
*(Or use the compiled launcher `./target/release/GmicVideo`)*

---

## 📦 AppImage Packaging

An automated build script is provided in the `packaging/` directory:

```bash
chmod +x packaging/build_appimage.sh
./packaging/build_appimage.sh
```

This compiles the release binary, bundles the application, desktop entry, and icon, packages it into a standalone `.AppImage` file, and executes a self-test to verify startup.

---

## 🎨 Custom Application Icon

To customize the application and window icon:
1. Navigate to the [`assets/icon/`](assets/icon/) directory.
2. Drop your custom artwork as `app_icon.png` (or `app_icon.svg`).
3. The app will automatically detect and load your icon on startup.

---

## 📄 License

<!-- FILL IN: Set your project license (e.g. GPL-3.0, MIT, etc.) -->
Distributed under the **GPL-3.0 License**. See `LICENSE` for more information.

---

## 🤝 Acknowledgments

<!-- FILL IN: Add credits or personal thanks -->
- [G'MIC Team (GREYC)](https://gmic.eu/) for the G'MIC image processing library.
- [KDE Kirigami](https://develop.kde.org/frameworks/kirigami/) for the desktop UI components.
- [FFmpeg](https://ffmpeg.org/) for multimedia extraction and video encoding.
