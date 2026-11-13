"""Records one window as an animated PNG: the engine's own window, and nothing else.

A recording for a demo or the visual history must never show the desktop or another program's
window (they are the operator's own). This recorder asks Windows for the contents of ONE window
(`PrintWindow` on its handle), a frame at a time, so nothing beside or over it can be in the
picture: not the desktop, not a window dragged across it, not a notification.

    rec = WindowRecorder(hwnd, fps=10)
    rec.start()
    ...drive the window...
    info = rec.stop(out / "clip.png")   # an APNG: browsers and most viewers play it

Standard library only (ctypes, zlib). Frames are compressed as they are taken and held in
memory (about 150 KiB each for a 1280x800 editor window), at most `max_frames`; a frame taken
while the window has another size is left out.
"""
import ctypes
import struct
import sys
import threading
import time
import zlib
from ctypes import wintypes
from pathlib import Path


def _chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def grab(hwnd):
    """The window's client area as rendered: (width, height, RGB rows with PNG filter bytes)."""
    user32, gdi32 = ctypes.windll.user32, ctypes.windll.gdi32
    r = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    if w <= 0 or h <= 0:
        return None
    hdc_window = user32.GetDC(hwnd)
    hdc = gdi32.CreateCompatibleDC(hdc_window)
    bitmap = gdi32.CreateCompatibleBitmap(hdc_window, w, h)
    gdi32.SelectObject(hdc, bitmap)
    ok = user32.PrintWindow(hwnd, hdc, 3)  # PW_RENDERFULLCONTENT: what the window drew, not the screen
    header = struct.pack("<IiiHHIIiiII", 40, w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(hdc, bitmap, 0, h, pixels, ctypes.c_char_p(header), 0)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(hdc)
    user32.ReleaseDC(hwnd, hdc_window)
    if not ok:
        return None
    px = pixels.raw
    rgb = bytearray(w * h * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = px[2::4], px[1::4], px[0::4]
    stride = w * 3
    rows = b"".join(b"\x00" + bytes(rgb[y * stride:(y + 1) * stride]) for y in range(h))
    return w, h, rows


class WindowRecorder:
    def __init__(self, hwnd, fps=10, max_frames=600):
        if sys.platform != "win32":
            raise RuntimeError("the window recorder is Windows only for now")
        self.hwnd, self.period, self.max_frames = hwnd, 1.0 / max(1, min(fps, 30)), max_frames
        self.frames = []  # (seconds since the start, zlib data)
        self.size = None
        self.skipped = 0
        self._stop = threading.Event()
        self._thread = None

    def start(self):
        self._stop.clear()
        self._thread = threading.Thread(target=self._run, name="window-recorder", daemon=True)
        self._thread.start()

    def _run(self):
        t0 = time.perf_counter()
        while not self._stop.is_set() and len(self.frames) < self.max_frames:
            began = time.perf_counter()
            got = grab(self.hwnd)
            if got:
                w, h, rows = got
                if self.size is None:
                    self.size = (w, h)
                if (w, h) == self.size:
                    self.frames.append((began - t0, zlib.compress(rows, 2)))
                else:
                    self.skipped += 1
            else:
                self.skipped += 1
            self._stop.wait(max(0.0, self.period - (time.perf_counter() - began)))

    def stop(self, path):
        """Stops and writes the APNG. Returns {frames, seconds, bytes, size, skipped}."""
        self._stop.set()
        if self._thread:
            self._thread.join(10)
        if not self.frames:
            return {"frames": 0, "seconds": 0.0, "bytes": 0, "size": None, "skipped": self.skipped}
        w, h = self.size
        times = [t for t, _ in self.frames]
        seconds = times[-1] + self.period
        out = bytearray(b"\x89PNG\r\n\x1a\n")
        out += _chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        out += _chunk(b"acTL", struct.pack(">II", len(self.frames), 0))
        seq = 0
        for i, (t, data) in enumerate(self.frames):
            shown = (times[i + 1] - t) if i + 1 < len(times) else self.period
            delay_ms = max(10, min(65535, round(shown * 1000)))
            out += _chunk(b"fcTL", struct.pack(">IIIIIHHBB", seq, w, h, 0, 0, delay_ms, 1000, 0, 0))
            seq += 1
            if i == 0:
                out += _chunk(b"IDAT", data)
            else:
                out += _chunk(b"fdAT", struct.pack(">I", seq) + data)
                seq += 1
        out += _chunk(b"IEND", b"")
        Path(path).write_bytes(bytes(out))
        return {"frames": len(self.frames), "seconds": round(seconds, 2), "bytes": len(out), "size": [w, h], "skipped": self.skipped}
