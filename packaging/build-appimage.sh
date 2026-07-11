#!/usr/bin/env bash
# Builds batista-gpu-benchmark-x86_64.AppImage from the release binary.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=target/release/batista-gpu-benchmark
[ -f "$BIN" ] || { echo "build first: cargo build --release"; exit 1; }

APPDIR=target/appimage/AppDir
rm -rf target/appimage
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/batista-gpu-benchmark" \
         "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps"

install -m755 "$BIN" "$APPDIR/usr/bin/"
cp -r assets "$APPDIR/usr/share/batista-gpu-benchmark/"
cp packaging/batista-gpu-benchmark.desktop "$APPDIR/usr/share/applications/"
cp packaging/batista-gpu-benchmark.desktop "$APPDIR/"
cp packaging/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/batista-gpu-benchmark.png"
cp packaging/icon.png "$APPDIR/batista-gpu-benchmark.png"
cp packaging/icon.png "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/batista-gpu-benchmark" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL=target/appimage/appimagetool
if [ ! -f "$TOOL" ]; then
    curl -fL -o "$TOOL" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$TOOL"
fi

ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" \
    target/appimage/batista-gpu-benchmark-x86_64.AppImage
echo "OK: target/appimage/batista-gpu-benchmark-x86_64.AppImage"
