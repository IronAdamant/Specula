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


class Mcp:
    """Minimal MCP client over Streamable HTTP (JSON responses)."""

    def __init__(self, url, client_name):
        self.url, self.session, self.next_id = url, None, 1
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
        try:
            reply = self.json("agent_hello", name=name, role=role, **({"resume_token": token} if token else {}))
        except RuntimeError as e:
            if token and "bad_resume_token" in str(e):  # a new engine run: start a new identity
                reply = self.json("agent_hello", name=name, role=role)
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

    def __init__(self, exe, port, log_path, *args, cwd=None):
        self.exe, self.port, self.log_path, self.args, self.cwd = Path(exe), port, Path(log_path), list(args), cwd
        self.proc, self.startup_s, self.shutdown_log, self.exit_code = None, None, "", None

    def __enter__(self):
        if server_up(self.port):
            raise RuntimeError(f"port {self.port} is already in use; pick another with --port")
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self.log = open(self.log_path, "w")
        flags = subprocess.CREATE_NEW_PROCESS_GROUP if sys.platform == "win32" else 0
        start = time.perf_counter()
        self.proc = subprocess.Popen([str(self.exe), "--port", str(self.port), *self.args], stderr=self.log,
                                     stdout=subprocess.DEVNULL, creationflags=flags, cwd=self.cwd)
        while not server_up(self.port):
            if self.proc.poll() is not None:
                self.log.close()
                raise RuntimeError(f"engine exited early:\n{self.log_path.read_text()}")
            if time.perf_counter() - start > 60:
                raise RuntimeError("engine's MCP server never came up")
            time.sleep(0.05)
        self.startup_s = time.perf_counter() - start
        return self

    def mcp(self, client_name):
        return Mcp(url_for(self.port), client_name)

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
