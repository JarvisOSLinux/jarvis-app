#!/usr/bin/env python3
"""Generate icon-source.png: a 1024x1024 dark navy + cyan ring icon.

Run once, then feed the output into Tauri's icon generator:
    python3 src-tauri/icons/generate_icon_source.py
    cargo tauri icon src-tauri/icons/icon-source.png

The second command produces icons/32x32.png, 128x128.png, 128x128@2x.png,
icon.icns, and icon.ico -- the files referenced by bundle.icon in
tauri.conf.json.
"""
import os
import struct
import zlib

SIZE = 1024
BG = (10, 14, 26, 255)       # #0a0e1a
CYAN = (0, 200, 255, 255)    # #00c8ff


def make_pixels():
    cx = cy = SIZE / 2
    outer_r = SIZE * 0.42
    ring_thickness = SIZE * 0.07
    inner_r = SIZE * 0.10
    rows = []
    for y in range(SIZE):
        row = bytearray()
        for x in range(SIZE):
            dx, dy = x - cx, y - cy
            dist = (dx * dx + dy * dy) ** 0.5
            if abs(dist - outer_r) <= ring_thickness / 2 or dist <= inner_r:
                px = CYAN
            else:
                px = BG
            row.extend(px)
        rows.append(bytes(row))
    return rows


def write_png(path, rows):
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data +
                struct.pack(">I", zlib.crc32(tag + data) & 0xffffffff))

    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    raw = b''.join(b'\x00' + r for r in rows)
    idat = zlib.compress(raw, 9)
    with open(path, 'wb') as f:
        f.write(sig)
        f.write(chunk(b'IHDR', ihdr))
        f.write(chunk(b'IDAT', idat))
        f.write(chunk(b'IEND', b''))


if __name__ == "__main__":
    out_path = os.path.join(os.path.dirname(__file__), "icon-source.png")
    write_png(out_path, make_pixels())
    print(f"wrote {out_path}")
