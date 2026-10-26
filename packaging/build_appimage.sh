#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APPDIR="$SCRIPT_DIR/AppDir"
OUTPUT_APPIMAGE="$ROOT_DIR/GmicVideo-x86_64.AppImage"
TOOLS_DIR="$SCRIPT_DIR/tools"
APPIMAGETOOL="$TOOLS_DIR/appimagetool"

echo "=========================================="
echo " Building G'MIC Video AppImage"
echo " Project root: $ROOT_DIR"
echo "=========================================="

# 1. Compile Rust backend in release mode
echo "[1/6] Compiling Rust backend binary..."
cd "$ROOT_DIR"
cargo build --release

BIN_SRC="$ROOT_DIR/target/release/GmicVideo"
if [ ! -f "$BIN_SRC" ]; then
    echo "Error: Binary not found at $BIN_SRC" >&2
    exit 1
fi

# Run thumbnail test
echo "[1.5/6] Verifying thumbnails and i18n tests..."
python3 "$ROOT_DIR/Tests/test_visual_search_thumbnails.py" && python3 "$ROOT_DIR/Tests/test_halftone_effect.py"

# 2. Prepare AppDir structure
echo "[2/6] Preparing AppDir structure..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/lib/gmic"
mkdir -p "$APPDIR/usr/share/gmicvideo"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Copy binary & assets
cp "$BIN_SRC" "$APPDIR/usr/bin/GmicVideo"
cp -r "$ROOT_DIR/src_gui" "$APPDIR/usr/share/gmicvideo/"
cp -r "$ROOT_DIR/ui" "$APPDIR/usr/share/gmicvideo/"
cp -r "$ROOT_DIR/assets" "$APPDIR/usr/share/gmicvideo/"

# Copy embedded G'MIC 4.0.6 binary and runtime libraries
if [ -d "$ROOT_DIR/bin" ]; then
    cp -r "$ROOT_DIR/bin/"* "$APPDIR/usr/bin/"
fi
if [ -d "$ROOT_DIR/lib/gmic" ]; then
    cp -r "$ROOT_DIR/lib/gmic/"* "$APPDIR/usr/lib/gmic/"
fi
# Effect thumbnails live in assets/thumbnails (copied with assets/ above); generate them with
#   ./target/release/GmicVideo --generate-thumbnails
if [ ! -d "$ROOT_DIR/assets/thumbnails" ]; then
    echo "Warning: assets/thumbnails missing - visual search will be empty" >&2
fi

# Copy icon
ICON_SRC="$ROOT_DIR/assets/icon/app_icon.png"
if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$APPDIR/usr/share/icons/hicolor/256x256/apps/gmicvideo.png"
    cp "$ICON_SRC" "$APPDIR/gmicvideo.png"
    cp "$ICON_SRC" "$APPDIR/.DirIcon"
fi

# 3. Create desktop file
echo "[3/6] Generating desktop entry..."
cat << 'DESKTOP_EOF' > "$APPDIR/gmicvideo.desktop"
[Desktop Entry]
Name=G'MIC Video
GenericName=Video Effects Editor
Comment=Non-linear video effects editor using Rust, G'MIC, and Kirigami
Exec=AppRun %F
Icon=gmicvideo
Terminal=false
Type=Application
Categories=AudioVideo;Video;AudioVideoEditing;KDE;Qt;
StartupNotify=true
DESKTOP_EOF

cp "$APPDIR/gmicvideo.desktop" "$APPDIR/usr/share/applications/gmicvideo.desktop"

# 4. Create AppRun script
echo "[4/6] Creating AppRun executable..."
cat << 'APPRUN_EOF' > "$APPDIR/AppRun"
#!/bin/bash
set -e

HERE="$(dirname "$(readlink -f "$0")")"
export APPDIR="$HERE"
export PATH="$HERE/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib/gmic:${LD_LIBRARY_PATH:-}"
export GMIC_BIN="$HERE/usr/bin/gmic"
export GMIC_VIDEO_BACKEND_BIN="$HERE/usr/bin/GmicVideo"
export GMIC_VIDEO_GUI_SCRIPT="$HERE/usr/share/gmicvideo/src_gui/gmicvideo_gui.py"

# Handle CLI flags or headless mode
if [ "$#" -gt 0 ]; then
    case "$1" in
        -h|--help|-v|--version|--server|--list-effects|--search|-i|--input)
            exec "$HERE/usr/bin/GmicVideo" "$@"
            ;;
        --test)
            exec python3 "$HERE/usr/share/gmicvideo/src_gui/gmicvideo_gui.py" "$@"
            ;;
    esac
fi

# Launch GUI
exec python3 "$HERE/usr/share/gmicvideo/src_gui/gmicvideo_gui.py" "$@"
APPRUN_EOF

chmod +x "$APPDIR/AppRun"

# 5. Acquire appimagetool and package AppImage
echo "[5/6] Packaging AppImage using appimagetool..."
mkdir -p "$TOOLS_DIR"
if [ ! -f "$APPIMAGETOOL" ]; then
    echo "Downloading appimagetool..."
    curl -L -s -o "$APPIMAGETOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$APPIMAGETOOL"
fi

rm -f "$OUTPUT_APPIMAGE"
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$OUTPUT_APPIMAGE"
chmod +x "$OUTPUT_APPIMAGE"

echo "AppImage created at: $OUTPUT_APPIMAGE"

# 6. Test if the app can open
echo "[6/6] Testing if the AppImage can open..."
QT_QPA_PLATFORM=offscreen "$OUTPUT_APPIMAGE" --test

echo "=========================================="
echo " ✅ SUCCESS! AppImage built and verified:"
echo " $OUTPUT_APPIMAGE"
echo "=========================================="
