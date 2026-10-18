"""A 96x64 test picture for the JPEG known-answer test: gradients, hard edges, a disc and saturated stripes."""
import math, struct, sys, zlib
W, H = 96, 64
rows = []
for y in range(H):
    row = bytearray([0])
    for x in range(W):
        r = int(255 * x / (W - 1))
        g = int(255 * y / (H - 1))
        b = 128
        if (x // 12 + y // 12) % 2 == 0 and y < 32:
            b = 220
        if (x - 64) ** 2 + (y - 40) ** 2 < 14 ** 2:
            r, g, b = 240, 200, 40
        if y >= 52 and (x // 4) % 3 == 0:
            r, g, b = 200, 30, 60
        elif y >= 52 and (x // 4) % 3 == 1:
            r, g, b = 30, 160, 230
        row += bytes([r, g, b])
    rows.append(bytes(row))
def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0)) + \
      chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
