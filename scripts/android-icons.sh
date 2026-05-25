#!/usr/bin/env bash
# Copy custom Android icon resources into the dx-generated build directory.
# dx regenerates the adaptive icon drawables (ic_launcher_foreground.xml,
# ic_launcher_background.xml) using its built-in Android-robot template on
# every build, so this script must be run after each `dx serve` or `dx bundle`.
#
# Usage: ./scripts/android-icons.sh [debug|release]   (default: debug)

set -euo pipefail
PROFILE="${1:-debug}"
SRC="packages/mobile/assets/android/res"
DEST="target/dx/mobile/${PROFILE}/android/app/app/src/main/res"

if [ ! -d "$DEST" ]; then
  echo "ERROR: $DEST does not exist. Run 'dx serve --package mobile' first." >&2
  exit 1
fi

echo "Copying icon resources → $DEST"

# Raster mipmap icons — convert our PNG to WebP so we replace dx's file
# in-place (same name, same format). Avoids the duplicate-resource error that
# occurs when both ic_launcher.webp and ic_launcher.png exist side-by-side.
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  mkdir -p "$DEST/mipmap-${density}"
  rm -f "$DEST/mipmap-${density}/ic_launcher.png"
  magick "$SRC/mipmap-${density}/ic_launcher.png" "$DEST/mipmap-${density}/ic_launcher.webp"
  echo "  mipmap-${density}/ic_launcher.webp"
done

# Adaptive icon drawables (background colour + foreground bitmap)
mkdir -p "$DEST/drawable" "$DEST/drawable-v24"
cp "$SRC/drawable/ic_launcher_background.xml"    "$DEST/drawable/ic_launcher_background.xml"
cp "$SRC/drawable/ic_launcher_fg.png"            "$DEST/drawable/ic_launcher_fg.png"
cp "$SRC/drawable-v24/ic_launcher_foreground.xml" "$DEST/drawable-v24/ic_launcher_foreground.xml"
echo "  drawable/ic_launcher_background.xml"
echo "  drawable-v24/ic_launcher_foreground.xml"

echo "Done. Reinstall the APK to see the new icon."
