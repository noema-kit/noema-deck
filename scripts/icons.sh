#!/bin/sh
# Renders every icon size from the master, `crates/noema-deck/icons/icon.svg`.
# Needs `rsvg-convert`, and on macOS `iconutil` for the `.icns` file.
set -eu
cd "$(dirname "$0")/../crates/noema-deck/icons"

render() {
    rsvg-convert --width "$1" --height "$1" icon.svg --output "$2"
}

render 1024 icon.png
render 32 32x32.png
render 128 128x128.png
render 256 128x128@2x.png

if command -v iconutil >/dev/null; then
    set=$(mktemp -d)/icon.iconset
    mkdir "$set"
    for size in 16 32 128 256 512; do
        render "$size" "$set/icon_${size}x${size}.png"
        render "$((size * 2))" "$set/icon_${size}x${size}@2x.png"
    done
    iconutil --convert icns --output icon.icns "$set"
    rm -r "$set"
fi
