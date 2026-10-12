#!/usr/bin/env python3
"""Window-path regression check for the Specula engine (Windows, needs a GPU and a desktop).

For every Vulkan adapter: start `specula` windowed, build a small scene over MCP, screenshot the
real window, resize / minimize / restore it, screenshot again, collect validation messages through
`sight_frame`, close it the way a user does (WM_CLOSE), and require a clean shutdown.

CI cannot run this (no GPU, no desktop). Run it after changes to specula-gpu, specula-platform or
the engine's window loop:

    cargo build -p specula
    python scripts/window_check.py                    # all adapters
    python scripts/window_check.py --adapters 1       # one adapter

Screenshots and engine logs go to target/window-check/ — open the PNGs and look at them.
Standard library only (ctypes for Win32, zlib for PNG); it ships nothing and runs out of process,
so it doesn't conflict with the engine's raw-dylib imports.
"""
import argparse
import ctypes
import json
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
import zlib
from ctypes import wintypes
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Its own port by default, so the check can never touch an engine you are working in (7471).
DEFAULT_PORT = 7491
URL = f"http://127.0.0.1:{DEFAULT_PORT}/mcp"

WM_CLOSE = 0x0010
SW_MINIMIZE, SW_RESTORE = 6, 9
SWP_NOMOVE, SWP_NOZORDER = 0x0002, 0x0004
PW_CLIENTONLY_RENDERFULLCONTENT = 3

if sys.platform != "win32":
    sys.exit("window_check: Windows only")
user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32")
# Measure in physical pixels, like the engine (per-monitor DPI aware v2).
user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))


class Mcp:
    """Minimal MCP client over Streamable HTTP (JSON responses)."""

    def __init__(self, client_name):
        self.session, self.next_id = None, 1
        self.request("initialize", {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": client_name, "version": "0"},
        })
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, msg):
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        req = urllib.request.Request(URL, json.dumps(msg).encode(), headers)
        with urllib.request.urlopen(req, timeout=60) as r:
            self.session = self.session or r.headers.get("Mcp-Session-Id")
            body = r.read()
        return json.loads(body) if body else None

    def request(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def call(self, tool, **args):
        result = self.request("tools/call", {"name": tool, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(f"{tool}: {result['content'][0]['text']}")
        return result

    def frame_warnings(self):
        result = self.call("sight_frame", width=640, height=360)
        packet = json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))
        return packet["warnings"]


def server_up():
    try:
        urllib.request.urlopen(urllib.request.Request(URL, method="GET"), timeout=1)
    except urllib.error.HTTPError:
        return True  # 405 from our server
    except OSError:
        return False
    return True


def find_window(pid):
    found = []
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    user32.EnumWindows(proto(visit), 0)
    return found[0] if found else None


def client_size(hwnd):
    r = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    return r.right - r.left, r.bottom - r.top


def screenshot(hwnd, path):
    """Client area as rendered (PrintWindow with PW_RENDERFULLCONTENT) → RGB PNG."""
    w, h = client_size(hwnd)
    hdc_window = user32.GetDC(hwnd)
    hdc = gdi32.CreateCompatibleDC(hdc_window)
    bitmap = gdi32.CreateCompatibleBitmap(hdc_window, w, h)
    gdi32.SelectObject(hdc, bitmap)
    ok = user32.PrintWindow(hwnd, hdc, PW_CLIENTONLY_RENDERFULLCONTENT)
    header = struct.pack("<IiiHHIIiiII", 40, w, -h, 1, 32, 0, 0, 0, 0, 0, 0)  # top-down BGRA
    pixels = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(hdc, bitmap, 0, h, pixels, ctypes.c_char_p(header), 0)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(hdc)
    user32.ReleaseDC(hwnd, hdc_window)

    raw = bytearray()
    for y in range(h):
        row = pixels.raw[y * w * 4:(y + 1) * w * 4]
        raw.append(0)  # filter: none
        for x in range(0, w * 4, 4):
            raw += bytes((row[x + 2], row[x + 1], row[x]))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 6)) + chunk(b"IEND", b"")
    path.write_bytes(png)
    colours = len({pixels.raw[i:i + 3] for i in range(0, len(pixels.raw), 4 * 97)})
    return ok != 0 and colours > 2  # captured, and not a blank/solid frame


def check_adapter(exe, adapter, out, port):
    """Returns True (clean), False (problem) or None (no such adapter)."""
    log_path = out / f"engine_adapter{adapter}.log"
    with open(log_path, "w") as log:
        proc = subprocess.Popen(
            [str(exe), "--adapter", str(adapter), "--size", "960x540", "--port", str(port), "--captures", str(out / "captures")],
            stderr=log,
        )
    try:
        for _ in range(150):
            if proc.poll() is not None:
                text = log_path.read_text()
                if "out of range" in text:
                    return None
                print(f"adapter {adapter}: engine exited early:\n{text}")
                return False
            if server_up():
                break
            time.sleep(0.2)
        else:
            print(f"adapter {adapter}: MCP server never came up")
            return False

        problems = []
        mcp = Mcp("window-check")
        info = json.loads(mcp.call("engine_info")["content"][0]["text"])
        print(f"adapter {adapter}: {info['gpu']['name']} (validation {'on' if info['validation'] else 'OFF'})")
        if not info["validation"]:
            problems.append("validation layer not active: install the Vulkan SDK for a meaningful check")
        mcp.call("world_spawn", shape="quad", name="ground", scale=[8, 1, 8], color="#3a5f3a")
        mcp.call("world_spawn", shape="cube", name="tower", position=[0, 0.75, 0], scale=[1.2, 1.5, 1.2], color="#8a8f99")
        mcp.call("world_spawn", shape="sphere", name="orb", position=[0, 2.0, 0], scale=0.8, color="#ffb02e")
        mcp.call("world_spawn", shape="cube", name="crate", position=[2.2, 0.3, 1.2], rotation_deg=[0, 30, 0], scale=0.6, color="#a0522d")
        warnings = mcp.frame_warnings()

        time.sleep(1.0)  # let the window present a few frames
        hwnd = None
        for _ in range(50):
            hwnd = find_window(proc.pid)
            if hwnd:
                break
            time.sleep(0.1)
        if not hwnd:
            print(f"adapter {adapter}: engine window not found")
            return False
        shots = [out / f"window_adapter{adapter}_initial.png", out / f"window_adapter{adapter}_restored.png"]
        if not screenshot(hwnd, shots[0]):
            problems.append(f"initial window capture is blank: {shots[0]}")
        sizes = [client_size(hwnd)]

        user32.SetWindowPos(hwnd, None, 0, 0, 1240, 760, SWP_NOMOVE | SWP_NOZORDER)
        time.sleep(1.0)
        sizes.append(client_size(hwnd))
        user32.ShowWindow(hwnd, SW_MINIMIZE)
        time.sleep(1.0)
        sizes.append(client_size(hwnd))
        user32.ShowWindow(hwnd, SW_RESTORE)
        time.sleep(1.5)
        sizes.append(client_size(hwnd))
        if not screenshot(hwnd, shots[1]):
            problems.append(f"window capture after restore is blank: {shots[1]}")
        print(f"  client sizes: {' -> '.join(f'{w}x{h}' for w, h in sizes)}")
        if sizes[2] != (0, 0) or sizes[3] != sizes[1]:
            problems.append(f"unexpected size sequence {sizes}")
        warnings += mcp.frame_warnings()  # includes everything raised while presenting
        if warnings:
            problems.append(f"validation warnings: {warnings}")

        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
        code = proc.wait(timeout=20)
        last = log_path.read_text().strip().splitlines()[-1]
        if code != 0 or "shut down (0 validation errors)" not in last:
            problems.append(f"shutdown: exit code {code}, log: {last}")
        print(f"  screenshots: {shots[0].name}, {shots[1].name}")
        for p in problems:
            print(f"  PROBLEM: {p}")
        print(f"  {'CLEAN' if not problems else 'NOT CLEAN'}")
        return not problems
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", type=Path, default=ROOT / "target" / "debug" / "specula.exe")
    parser.add_argument("--adapters", default="all", help="'all' or comma-separated indices, e.g. 0,1")
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "window-check")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help=f"engine port (default {DEFAULT_PORT})")
    args = parser.parse_args()
    global URL
    URL = f"http://127.0.0.1:{args.port}/mcp"
    if not args.exe.is_file():
        sys.exit(f"window_check: {args.exe} not found (cargo build -p specula)")
    if server_up():
        sys.exit(f"window_check: something is already serving on port {args.port}; stop it or pass --port")
    args.out.mkdir(parents=True, exist_ok=True)

    indices = range(16) if args.adapters == "all" else [int(i) for i in args.adapters.split(",")]
    results = {}
    for i in indices:
        result = check_adapter(args.exe, i, args.out, args.port)
        if result is None:
            if args.adapters != "all":
                results[i] = False
                print(f"adapter {i}: no such adapter")
            break
        results[i] = result
    if not results:
        sys.exit("window_check: no adapters checked")
    clean = all(results.values())
    print(f"\n{sum(results.values())}/{len(results)} adapters clean; screenshots in {args.out}")
    sys.exit(0 if clean else 1)


if __name__ == "__main__":
    main()
