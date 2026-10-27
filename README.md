# G'MIC Video Effects Editor

[![Release](https://img.shields.io/github/v/release/IsaqueS/GmicVideo)](https://github.com/IsaqueS/GmicVideo/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/Python-3.10+-blue.svg)](https://www.python.org/)
[![Framework](https://img.shields.io/badge/UI-KDE%20Kirigami%20%7C%20PySide6-blueviolet.svg)](https://develop.kde.org/frameworks/kirigami/)
[![License](https://img.shields.io/badge/License-GPLv3-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux-lightgrey.svg)](https://www.kernel.org/)

A high-performance, non-linear video effects editor powered by **Rust**, **G'MIC**, **FFmpeg**, and the **KDE Kirigami Framework**. Designed as the spiritual successor to `ultrawarp-batch`, G'MIC Video combines the full image-processing power of over 1,100 G'MIC filters with parameter tweening, mathematical curve sculpting, and multi-threaded parallel rendering.

---

## ⬇️ Download

Grab the latest **AppImage** from the [Releases page](https://github.com/IsaqueS/GmicVideo/releases/latest):

```bash
chmod +x GmicVideo-*-x86_64.AppImage
./GmicVideo-*-x86_64.AppImage
```

Each release also has a `.sha256` file (`sha256sum -c GmicVideo-*.sha256`).

The AppImage bundles the Rust backend, the UI and G'MIC 4.0.6, but uses these from your system:
- `python3` with **PySide6**
- **KDE Kirigami** (`kf6-kirigami` / `qml6-module-org-kde-kirigami`)
- `ffmpeg` and `ffprobe`
- *(optional)* a Vulkan driver for the GPU effects

---

## ✨ Features

### ▶️ Real-Time Playback
- Play the video with the effect applied, with audio (Space to play/pause, mute, loop).
- Slow effects skip frames instead of lagging behind the audio.
- **Preview Resolution** option on every effect (Auto, 25%, 50%, 75%, Full). *Auto* lowers the
  resolution while frames are dropped and raises it again when there is headroom. The final render
  always uses the full resolution.
- Change parameters while the video plays; the next frames use the new values.

### ⚡ GPU Effects (Vulkan)
G'MIC itself only runs on the CPU, so the most-used effects were re-implemented as Vulkan compute
shaders (through `wgpu`). They read the same parameters as G'MIC, so tweens, curves and `.geffect`
presets work unchanged, and each one is tested against G'MIC's own output. Effects with a GPU version
show a green **GPU** badge in the effect list; all other effects keep running in G'MIC.
- **Hand-written ports:** UltraWarp 2, UltraWarp 3, Halftone, Vignette, Blur [Gaussian],
  Sharpen [Unsharp Mask], Noise [Additive], VHS Filter.
- **Deformations as warp maps:** 38 deformations (Twirl, Fish-Eye, Wave, Ripple, Sphere, Spherize,
  Kaleidoscope [Symmetry], Polar transforms, …) run G'MIC once on a coordinate image and the GPU then
  re-applies the result to every frame. Which deformations qualify is decided by
  `--classify-warps` (see below) and stored in `assets/gpu/warp_bakeable.json`.
- UltraWarp's random choices come from a generator seeded with the frame number, so frames are
  repeatable but differ from G'MIC's random sequence (the look is the same).
- No Vulkan GPU? Everything falls back to the CPU automatically
  (`GMIC_VIDEO_DISABLE_GPU=1` forces this).

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
│                  Backend Core (Rust 2024)                   │
│  - Persistent G'MIC worker processes (raw frames, no PNG)   │
│  - Vulkan GPU effects (wgpu compute shaders)                │
│  - Real-time playback engine (frame pipe to QML)            │
│  - FFmpeg & FFprobe: raw decoding, encoding & audio muxing  │
└─────────────────────────────────────────────────────────────┘
```

---

## 📋 System Requirements

- **OS**: Linux (Fedora, Ubuntu/Debian, Arch Linux, openSUSE, etc.)
- **Rust**: `1.75+` (with `cargo`)
- **Python**: `3.10+` with `PySide6`
- **KDE Frameworks**: `kf6-kirigami` (or `kf5-kirigami`)
- **System Binaries**:
  - `ffmpeg` & `ffprobe`
  - A bundled G'MIC 4.0.6 is included in `bin/` and `lib/gmic/`
- **GPU (optional)**: a Vulkan driver (Mesa RADV/ANV, NVIDIA, …) for the GPU effects

---

## 🚀 Building & Running

### 1. Clone Repository
Test images and thumbnails are stored with [Git LFS](https://git-lfs.com/), so install it first
(`git lfs install`).
```bash
git clone https://github.com/IsaqueS/GmicVideo.git
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

### 4. Optional maintenance commands
```bash
# Re-render the visual-search thumbnails (assets/thumbnails/, ~1 min per 100 effects)
./target/release/GmicVideo --generate-thumbnails [--force] [--thumbnail-timeout 30]

# Re-check which deformations can run on the GPU (writes assets/gpu/warp_bakeable.json)
./target/release/GmicVideo --classify-warps
```

### 5. Launch the Application
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

## 🚢 Releasing

`ship.sh` bumps the version, runs all tests, builds the AppImage and publishes a GitHub release:

```bash
./ship.sh                 # patch release (1.0.0 -> 1.0.1)
./ship.sh minor           # 1.0.0 -> 1.1.0
./ship.sh major           # 1.0.0 -> 2.0.0
./ship.sh 1.4.2           # explicit version
./ship.sh --notes "Fixed the halftone preview"   # custom release notes
./ship.sh --draft         # create the release as a draft
./ship.sh --dry-run minor # show what would happen, change nothing
```

Requirements: a clean working tree, `gh` logged in, `git-lfs`, and everything needed by
`packaging/build_appimage.sh`. Without `--notes`, the notes list the commits since the last tag.

---

## 🎨 Custom Application Icon

To customize the application and window icon:
1. Navigate to the [`assets/icon/`](assets/icon/) directory.
2. Drop your custom artwork as `app_icon.png` (or `app_icon.svg`).
3. The app will automatically detect and load your icon on startup.

---

## 📄 License

Distributed under the **GPL-3.0 License**. See [`LICENSE`](LICENSE) for more information.

The bundled G'MIC 4.0.6 (`bin/`, `lib/gmic/`, `assets/gmic/update406.gmic`) is licensed under
CeCILL / CeCILL-C — see [`THIRD_PARTY.md`](THIRD_PARTY.md).

---

## 🤝 Acknowledgments

- [G'MIC Team (GREYC)](https://gmic.eu/) for the G'MIC image processing library.
- [KDE Kirigami](https://develop.kde.org/frameworks/kirigami/) for the desktop UI components.
- [FFmpeg](https://ffmpeg.org/) for multimedia extraction and video encoding.
