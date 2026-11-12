"""Shared helpers for scripts that drive Specula: a minimal MCP client and an engine launcher.

Standard library only. Keep output ASCII (Python's -I flag ignores PYTHONIOENCODING).
"""
import json
import os
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def _physical_window_pixels():
    """This process speaks to windows in physical pixels (per-monitor DPI aware).

    Without it, on a display scaled by the OS (125%, 175%), Windows gives a script scaled-down
    window sizes and rescales the mouse messages it posts: a click posted at a widget's rect (which
    the engine reports in the window's real pixels) lands somewhere else, and only on machines
    whose display is scaled. Every script that touches a window imports this module, so it is set
    here once (a second call by a script that sets it itself fails harmlessly)."""
    if sys.platform != "win32":
        return
    try:
        import ctypes
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # PER_MONITOR_AWARE_V2
    except (AttributeError, OSError):
        pass


_physical_window_pixels()


# Tools that answer concisely by default (`response_format`, see docs/design/TOOLS.md). Scripts
# check engine data in depth, so this client asks them for detailed answers unless a call passes
# its own response_format (or the client is made with detailed=False, as an agent would see them).
# A script with its own MCP client wraps its arguments in `detailed(tool, args)`.
DETAILED_BY_DEFAULT = {"sight_frame", "perf_frame", "engine_info", "world_list", "world_query", "world_history", "play_state",
                       "logic_status", "playtest_run", "playtest_report"}


def detailed(tool, args):
    """`args` asking `tool` for its detailed answer (when it has a format and the call doesn't choose)."""
    if tool in DETAILED_BY_DEFAULT and "response_format" not in (args or {}):
        return dict(args or {}, response_format="detailed")
    return args


def reply_of(body):
    """The JSON-RPC message of a POST's answer body: plain JSON, or an event stream (the engine
    answers as one when it tells the session notifications/tools/list_changed first; the response
    is the event with an id). None for an empty body."""
    if not body:
        return None
    text = body.decode() if isinstance(body, bytes) else body
    if not text.lstrip().startswith(("event:", "data:", "id:", ":")):
        return json.loads(text)
    messages = [json.loads(line[5:].strip()) for line in text.splitlines() if line.startswith("data:") and line[5:].strip()]
    return next((m for m in reversed(messages) if "id" in m), messages[-1] if messages else None)


class Mcp:
    """Minimal MCP client over Streamable HTTP (JSON responses)."""

    def __init__(self, url, client_name, detailed=True, tools="all"):
        # Scripts list every tool (`?tools=all`): the engine's default for a client that says
        # nothing is the core tools only (docs/design/TOOLS.md, "Scoped tool lists"). Pass
        # tools=None to connect as an unconfigured agent would, or a URL with its own query.
        if tools and "?" not in url:
            url = f"{url}?tools={tools}"
        self.url, self.session, self.next_id = url, None, 1
        self.detailed = detailed
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
        req = urllib.request.Request(self.url, json.dumps(msg).encode(), headers)
        with urllib.request.urlopen(req, timeout=120) as r:
            self.session = self.session or r.headers.get("Mcp-Session-Id")
            body = r.read()
        return reply_of(body)

    def request(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def call(self, tool, **args):
        if self.detailed:
            args = detailed(tool, args)
        result = self.request("tools/call", {"name": tool, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(f"{tool}: {result['content'][0]['text']}")
        return result

    def text(self, tool, **args):
        """The first text content of a tool call."""
        result = self.call(tool, **args)
        return next(c["text"] for c in result["content"] if c["type"] == "text")

    def json(self, tool, **args):
        return json.loads(self.text(tool, **args))

    def frame(self, **args):
        return self.json("sight_frame", **args)

    def hello(self, name, role="builder", token_file=None):
        """agent_hello that survives script restarts: with `token_file`, the resume token from the
        first run is stored there and later runs resume the same identity (claims and undo come
        along) instead of being refused as name_taken."""
        token = None
        if token_file and os.path.exists(token_file):
            token = open(token_file).read().strip() or None
        for attempt in range(4):
            try:
                reply = self.json("agent_hello", name=name, role=role, **({"resume_token": token} if token else {}))
                break
            except RuntimeError as e:
                if token and "bad_resume_token" in str(e):  # a new engine run: start a new identity
                    token = None
                elif "engine_busy" in str(e) and attempt < 3:
                    # A cold engine on a loaded machine can take over 30 s to answer its first
                    # call (it says so, with retry: true, and applied nothing): ask again.
                    print(f"  (agent_hello: the engine was busy starting; asking again, try {attempt + 2} of 4)")
                else:
                    raise
        if token_file and reply.get("resume_token"):
            with open(token_file, "w") as f:
                f.write(reply["resume_token"])
        return reply

    def close(self):
        """Ends the MCP session (DELETE), so its name is free at once. Claims stay with the
        identity while another session carries it (see hello's token_file)."""
        if not self.session:
            return
        req = urllib.request.Request(self.url, method="DELETE", headers={"Mcp-Session-Id": self.session})
        try:
            urllib.request.urlopen(req, timeout=10).read()
        except OSError:
            pass
        self.session = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False

    def tools(self):
        return [t["name"] for t in self.request("tools/list", {})["tools"]]


def share_file(mcp, path):
    """Copies a file (another engine's desync trace, say) into the captures folder of the engine
    `mcp` talks to, and returns the name to pass it: engines read files agents name only under
    their own folders (captures, worlds, assets), relative to them."""
    import shutil
    captures = Path(mcp.json("engine_info", response_format="detailed")["captures"])
    captures.mkdir(parents=True, exist_ok=True)
    src = Path(path)
    if src.resolve().parent == captures.resolve():
        return src.name
    name = f"shared_{src.name}"
    shutil.copyfile(src, captures / name)
    return name


def url_for(port):
    return f"http://127.0.0.1:{port}/mcp"


def server_up(port):
    try:
        urllib.request.urlopen(urllib.request.Request(url_for(port), b"{}", {"Content-Type": "application/json"}),
                               timeout=1)
    except urllib.error.HTTPError:
        return True  # it answered (with an error for the empty request): it's up
    except OSError:
        return False
    return True


class Engine:
    """Starts `specula` on its own port and stops it cleanly (Ctrl+Break, like a user's Ctrl+C).

    Use as a context manager. `startup_s` is the time from launch until MCP answered;
    `shutdown_log` holds the engine's stderr after it stopped.
    """

    def __init__(self, exe, port, log_path, *args, cwd=None, env=None):
        self.exe, self.port, self.log_path, self.args, self.cwd = Path(exe), port, Path(log_path), list(args), cwd
        # A hosted game keeps no settings unless the script names a folder (--settings DIR). With
        # a window and no flag, the game would read the real per-user settings of whoever runs
        # the script (their saved reduced motion would skip an intro a check waits for), and the
        # keys and clicks a script posts to the window count as the player's and would be
        # written back into that file. (SPECULA_NO_SETTINGS_FLAG=1: for an older engine build
        # that doesn't know the flag, in an A/B run.)
        engine_args = self.args[: self.args.index("--")] if "--" in self.args else self.args
        if "--settings" not in engine_args and not os.environ.get("SPECULA_NO_SETTINGS_FLAG"):
            self.args[0:0] = ["--settings", "off"]
        # The engine's environment (default: this process's).
        self.env = env
        self.proc, self.startup_s, self.shutdown_log, self.exit_code = None, None, "", None

    def __enter__(self):
        if server_up(self.port):
            raise RuntimeError(f"port {self.port} is already in use; pick another with --port")
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self.log = open(self.log_path, "w")
        flags = subprocess.CREATE_NEW_PROCESS_GROUP if sys.platform == "win32" else 0
        start = time.perf_counter()
        self.proc = subprocess.Popen([str(self.exe), "--port", str(self.port), *self.args], stderr=self.log,
                                     stdout=subprocess.DEVNULL, creationflags=flags, cwd=self.cwd, env=self.env)
        # The process exists: a subclass may note it (before it answers, so a kill during start-up
        # can't hide it).
        self.started_process()
        while not server_up(self.port):
            if self.proc.poll() is not None:
                self.log.close()
                raise RuntimeError(f"engine exited early:\n{self.log_path.read_text()}")
            if time.perf_counter() - start > 60:
                # Don't leave it running: __exit__ won't run for a failed __enter__.
                self.proc.kill()
                self.proc.wait()
                self.log.close()
                raise RuntimeError("engine's MCP server never came up")
            time.sleep(0.05)
        self.startup_s = time.perf_counter() - start
        return self

    def started_process(self):
        """Called right after the engine's process starts (self.proc), before it answers."""

    def mcp(self, client_name, detailed=True):
        return Mcp(url_for(self.port), client_name, detailed)

    def stop(self):
        if self.proc is None or self.proc.poll() is not None:
            return
        if sys.platform == "win32":
            os.kill(self.proc.pid, signal.CTRL_BREAK_EVENT)
        else:
            self.proc.send_signal(signal.SIGINT)
        try:
            self.exit_code = self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.exit_code = self.proc.wait()

    def __exit__(self, *exc):
        self.stop()
        self.log.close()
        self.shutdown_log = self.log_path.read_text(errors="replace")
        return False

    def validation_errors(self):
        """From the engine's last line: `shut down (N validation errors)`; None if missing."""
        for line in reversed(self.shutdown_log.splitlines()):
            if "shut down (" in line:
                return int(line.split("shut down (")[1].split()[0])
        return None
