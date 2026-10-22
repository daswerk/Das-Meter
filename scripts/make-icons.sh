#!/usr/bin/env bash
# Makes Das-Meter's icons from crates/app/assets/icon/das-meter.svg (macOS,
# needs Google Chrome to render the SVG, and iconutil):
#
#   AppIcon.icns          the macOS app icon, every size Finder and the Dock use
#   das-meter.ico         Windows: the installer, shortcuts, Add/Remove Programs
#   das-meter-64.rgba     the window icon on Windows and Linux (64×64, raw RGBA)
#
# Windows icons fill their square, so they're cut from the tile itself,
# without the macOS margin and shadow. Run it after changing the SVG and
# commit what it writes.
#
#   scripts/make-icons.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
dir="$root/crates/app/assets/icon"
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Renders an SVG to a PNG `size` px square, keeping its transparency.
render() {
    local svg=$1 size=$2 out=$3
    printf '<html><body style="margin:0;background:transparent"><img src="file://%s" width="%s" height="%s"></body></html>' \
        "$svg" "$size" "$size" >"$work/page.html"
    "$chrome" --headless=new --disable-gpu --hide-scrollbars \
        --default-background-color=00000000 --window-size="$size,$size" \
        --user-data-dir="$work/profile" --screenshot="$out" "file://$work/page.html" >/dev/null 2>&1
}

# macOS: the tile with its margin and shadow, at every iconset size.
render "$dir/das-meter.svg" 1024 "$work/mac.png"
iconset="$work/AppIcon.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$work/mac.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$work/mac.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$dir/AppIcon.icns"

# Windows and Linux: just the tile, edge to edge.
sed -e 's|viewBox="0 0 1024 1024"|viewBox="100 100 824 824"|' -e 's| filter="url(#drop)"||' \
    "$dir/das-meter.svg" >"$work/square.svg"
render "$work/square.svg" 512 "$work/square.png"
for size in 16 24 32 48 64 128 256; do
    sips -z "$size" "$size" "$work/square.png" --out "$work/square-$size.png" >/dev/null
done
python3 - "$work" "$dir" <<'EOF'
import struct, sys, zlib

work, out = sys.argv[1], sys.argv[2]

def rgba(path):
    """Decodes an 8-bit RGB or RGBA, non-interlaced PNG into rows of RGBA bytes."""
    data = open(path, 'rb').read()
    pos, chunks = 8, []
    while pos < len(data):
        length, kind = struct.unpack('>I4s', data[pos:pos + 8])
        chunks.append((kind, data[pos + 8:pos + 8 + length]))
        pos += 12 + length
    header = dict(chunks)[b'IHDR']
    width, height, depth, colour = struct.unpack('>IIBB', header[:10])
    assert depth == 8 and colour in (2, 6), (depth, colour)
    channels = 4 if colour == 6 else 3
    raw = zlib.decompress(b''.join(d for k, d in chunks if k == b'IDAT'))
    stride = width * channels
    rows, previous = [], bytearray(stride)
    for y in range(height):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            left = line[i - channels] if i >= channels else 0
            up = previous[i]
            corner = previous[i - channels] if i >= channels else 0
            if kind == 1:
                line[i] = (line[i] + left) & 255
            elif kind == 2:
                line[i] = (line[i] + up) & 255
            elif kind == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif kind == 4:
                p = left + up - corner
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                predictor = left if pa <= pb and pa <= pc else up if pb <= pc else corner
                line[i] = (line[i] + predictor) & 255
        previous = line
        if channels == 3:
            line = bytearray(b''.join(bytes(line[i:i + 3]) + b'\xff' for i in range(0, stride, 3)))
        rows.append(bytes(line))
    return width, height, b''.join(rows)

# The window icon: 64×64 raw RGBA, as winit takes it.
width, height, pixels = rgba(f'{work}/square-64.png')
assert (width, height) == (64, 64)
open(f'{out}/das-meter-64.rgba', 'wb').write(pixels)

# The .ico: every size as an embedded PNG (Windows Vista and later).
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [open(f'{work}/square-{s}.png', 'rb').read() for s in sizes]
offset = 6 + 16 * len(sizes)
entries = b''
for size, image in zip(sizes, images):
    side = 0 if size == 256 else size
    entries += struct.pack('<BBBBHHII', side, side, 0, 0, 1, 32, len(image), offset)
    offset += len(image)
open(f'{out}/das-meter.ico', 'wb').write(
    struct.pack('<HHH', 0, 1, len(sizes)) + entries + b''.join(images))
EOF
echo "Wrote AppIcon.icns, das-meter.ico and das-meter-64.rgba to $dir"
