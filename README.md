# G'MIC Video Effects Editor

A modern Rust-based video effects editor and spiritual successor to `ultrawarp-batch`. Built using the **KDE Kirigami Framework**, **FFmpeg**, and **G'MIC** CLI image computing library.

---

## Features

- **KDE Kirigami UI**:
  - Video selection welcome screen for single-video import.
  - Three-column workspace:
    - **Left**: Category explorer & real-time search bar across 1,100+ G'MIC filters (including UltraWarp 2, UltraWarp++++, Halftone, Bloom, Pixel Art, Kaleidoscope, and more).
    - **Center**: Filter customization parameters with inline controls (Sliders, SpinBoxes, ComboBoxes, Switches).
    - **Right**: Video preview with full-view or side-by-side split comparison, and an interactive timeline scrubber.
- **Advanced Parameter Tweening & Curves**:
  - Every numerical parameter can have an animated **Tween** over the video's duration.
  - Built-in transitions: `linear`, `ease_in_quad`, `ease_out_quad`, `ease_in_out_cubic`, `elastic`, `bounce`, `back`, `sine`, `expo`, `circ`.
  - **Interactive Cubic Bezier Curve Editor**: Drag handles $P_1$ and $P_2$ on an interactive canvas or select presets (`Ease`, `Ease In`, `Ease Out`, `Ease In-Out`, `Overshoot`).
- **High-Performance Parallel Rendering**:
  - Multi-threaded frame processing using Rust's `rayon` work-stealing pool across all available CPU cores.
  - Live progress tracking dialog with completed frame count, encoding speed (FPS), elapsed time, and ETA.
- **Versatile Video Formats**:
  - Render with custom CRF / quality control to:
    - **H.264** (`libx264`, MP4)
    - **H.265 / HEVC** (`libx265`, MP4)
    - **VP9** (`libvpx-vp9`, WebM)
    - **AV1** (`libsvtav1`, MP4)
    - **Lossless Video** (`libx264rgb` CRF 0, RGB 4:4:4)

---

## Project Structure

```
├── Cargo.toml
├── src/
│   ├── main.rs         # CLI entry point, argument parsing & Kirigami launcher
│   ├── gmic.rs         # G'MIC command parser, filter database & CLI runner
│   ├── tween.rs        # Easing formulas, Bezier evaluation & curve interpolation
│   ├── video.rs        # FFmpeg & FFprobe frame extraction, probing & video encoding
│   ├── renderer.rs     # Multi-threaded Rayon batch rendering pipeline
│   └── server.rs       # JSON-over-stdio IPC server for Kirigami UI communication
├── src_gui/
│   └── gmicvideo_gui.py # PySide6 + Kirigami QML application loader & IPC bridge
└── ui/
    ├── main.qml            # Main Kirigami application window & effects panel
    ├── BezierCurveEditor.qml # Interactive cubic Bezier curve editor overlay sheet
    └── RenderDialog.qml    # Render progress and completion modal
```

---

## Running the Application

### 1. Launch the Kirigami GUI
Run from anywhere in your terminal:
```bash
gmicvideo
```
Or directly from the project directory:
```bash
./target/release/GmicVideo
```
*(Also accessible from your KDE Plasma Application Menu under Multimedia / Video)*

### 2. Command Line / Batch Mode
List available G'MIC filters:
```bash
gmicvideo --list-effects
```

Search for filters:
```bash
gmicvideo --search ultrawarp
```

Headless render a video with an effect:
```bash
gmicvideo --input input.mp4 --output output.mp4 --filter patterns_halftone --format h264 --crf 18
```
Supported formats: `h264`, `h265`, `vp9`, `av1`, `lossless`.
