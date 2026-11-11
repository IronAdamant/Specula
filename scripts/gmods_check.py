"""Live check of the gameplay modules (docs/design/GAMEPLAY_MODULES.md): an agent builds the four
example modules (inventarium, facultas, colloquium, quaestio), the walking character (ambulo) and the
demo scene (exemplum) into a running engine with logic_build, then plays the demo's quest through
with logic_input and logic_hook, as a player would:

  talk to the Sexton and take the quest -> pick up three bones -> banish two wisps with the lantern
  flare -> hand the bones over -> the quest completes, a silver key and the ward ability arrive.

Checked:
- every build is clean (no diagnostics, no determinism or security warnings) and loads;
- the run is deterministic across two engine runs: the same script in a second engine gives the same
  state hash at every checkpoint;
- the journal replays to the live hash (world_hash verify);
- a save written mid-quest loads and finishes with the same hash as the unbroken run;
- the modules' sounds play through the engine (audio_played by module:<name>; --audio null);
- logic_build refuses a module that declares foreign functions (native_code) or depends on an
  engine crate that calls the OS (os_crate): modules never read the window's keyboard directly;
- with --window, captures of the dialogue, the flare and the finished quest for review.

    cargo build -p specula && python scripts/gmods_check.py [--port 7640] [--window] [--adapter integrated]

Results go to <target>/gmods-check/ (summary.json, captures, engine logs), where <target> is
$CARGO_TARGET_DIR or ./target. ASCII output only. Fails loudly (exit 1) on the first failed check.
"""
import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

MODULES = ["ambulo", "inventarium", "facultas", "colloquium", "quaestio", "exemplum"]
SEED = 11
PROBE = ROOT / "games" / f"_gmods_probe_{os.getpid()}"


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        raise SystemExit(f"gmods_check: failed: {what}")


def build(mcp, path, **extra):
    """logic_build's answer and whether it was an error (failures are answers too)."""
    result = mcp.request("tools/call", {"name": "logic_build", "arguments": {"path": path, **extra}})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    try:
        return json.loads(text), result.get("isError", False)
    except json.JSONDecodeError:
        return {"raw": text}, True


class Player:
    """The script a player follows, through the agent tools."""

    def __init__(self, mcp, label):
        self.mcp, self.label, self.marks = mcp, label, []

    def step(self, n):
        self.mcp.json("time_control", action="step", ticks=n)

    def press(self, module, action):
        self.mcp.json("logic_input", module=module, actions={action: True}, ticks=1)
        self.step(1)
        self.mcp.json("logic_input", module=module, actions={action: False}, ticks=1)
        self.step(3)

    def go(self, x, z):
        self.mcp.json("world_set", path="/ambulo/player", position=[x, 0.0, z])
        self.step(2)

    def props(self):
        q = self.mcp.json("world_query", path_prefix="/ambulo", tags=["player"], fields=["props"])
        items = q.get("entities") or q.get("items") or []
        if len(items) != 1:
            check(False, f"[{self.label}] one player (got {len(items)})")
        return items[0].get("props") or {}

    def items(self, item):
        slots = (self.props().get("inventory") or {}).get("slots") or []
        return sum(s["count"] for s in slots if s and s.get("item") == item)

    def quest(self):
        return (self.props().get("quests") or {}).get("bones") or {}

    def node(self):
        return (self.props().get("colloquium") or {}).get("node")

    def mark(self, what):
        h = self.mcp.json("world_hash")
        self.marks.append({"at": what, "tick": h["tick"], "hash": h["state_hash"], "physics": (h.get("physics") or {}).get("state_hash")})
        return h

    def first_half(self, frame=None):
        self.go(2.2, 2.0)
        self.press("colloquium", "talk")
        check(self.node() == "greet", f"[{self.label}] the Sexton greets (node {self.node()})")
        if frame:
            frame("dialogue", eye=[0.4, 2.6, 6.8], target=[3.2, 2.0, 2.2])
        self.press("colloquium", "choice_1")
        check(self.node() == "offer", f"[{self.label}] 'Can I help?' -> offer")
        self.press("colloquium", "choice_1")
        check(self.node() == "accepted", f"[{self.label}] 'I'll find them.' -> accepted")
        self.press("colloquium", "talk")
        check(self.node() is None, f"[{self.label}] the talk ended")
        check(self.quest().get("state") == "active", f"[{self.label}] quest active ({self.quest()})")
        check(self.items("lantern") == 1, f"[{self.label}] the Sexton's lantern is in the inventory")
        self.mark("quest_taken")
        for x, z in [(-1.8, -3.0), (0.2, -3.2), (-3.6, -1.6)]:
            self.go(x, z)
            self.press("inventarium", "interact")
        check(self.items("bone") == 3, f"[{self.label}] three bones picked up ({self.items('bone')})")
        check(self.quest()["progress"]["collect"] == 3, f"[{self.label}] collect 3/3")
        if frame:
            frame("tracker", eye=[-3.4, 2.3, 0.8], target=[-3.9, 1.9, -3.0])
        return self.mark("bones")

    def second_half(self, frame=None):
        self.go(-0.5, -6.0)
        self.press("facultas", "ability_1")
        if frame:
            frame("flare", eye=[2.5, 2.8, -1.0], target=[-0.6, 1.0, -6.2])
        self.step(80)
        self.press("facultas", "ability_1")
        self.step(4)
        check(self.quest()["progress"]["banish"] == 2, f"[{self.label}] two wisps banished ({self.quest()['progress']})")
        self.mark("wisps")
        self.go(2.2, 2.0)
        self.press("colloquium", "talk")
        self.press("colloquium", "choice_1")
        check(self.node() == "progress", f"[{self.label}] 'About those bones...' -> progress")
        self.press("colloquium", "choice_1")
        check(self.node() == "thanks", f"[{self.label}] 'Here are three bones.' -> thanks")
        self.press("colloquium", "talk")
        self.step(4)
        q = self.quest()
        check(q.get("state") == "done", f"[{self.label}] quest done ({q})")
        check(self.items("bone") == 0 and self.items("silver_key") == 1, f"[{self.label}] bones handed over, silver key received")
        known = (self.props().get("abilities") or {}).get("known")
        check(known == ["flare", "ward"], f"[{self.label}] the ward was learned ({known})")
        if frame:
            frame("done", eye=[-3.4, 2.3, 0.8], target=[-3.9, 1.9, -3.0])
        return self.mark("done")


def build_all(mcp, label, summary):
    for m in MODULES:
        r, err = build(mcp, f"games/{m}", seed=SEED)
        diags = r.get("diagnostics") or []
        check(not err and r.get("ok"), f"[{label}] logic_build games/{m} ({r.get('error')})")
        check((r.get("reload") or {}).get("status") == "running", f"[{label}] {m} is running")
        check(len(diags) == 0, f"[{label}] {m}: no diagnostics ({[d.get('code') for d in diags][:6]})")
        summary.setdefault("builds_ms", {}).setdefault(label, {})[m] = (r.get("timings_ms") or {}).get("total")


def proposal_probe(engine, lead, summary):
    """A builder's logic_build builds nothing: it files a proposal the lead sees; the builder
    can't approve it, reload a library or unload a module."""
    b = engine.mcp("gmods-builder")
    b.hello("gmods-builder")
    r, err = build(b, "games/quaestio")
    check(err and r.get("error") == "lead_only" and isinstance(r.get("proposal"), dict), f"a builder's logic_build is a proposal ({str(r)[:200]})")
    pid = r["proposal"]["id"]
    r2, err2 = build(b, "games/quaestio", approve=pid)
    check(err2 and r2.get("error") == "lead_only", "the builder can't approve its own proposal")
    rl = b.request("tools/call", {"name": "logic_reload", "arguments": {"module": "quaestio"}})
    check(rl.get("isError") and "lead_only" in json.dumps(rl), "the builder can't load a library")
    listed = lead.json("logic_status").get("proposals") or []
    check(any(p.get("id") == pid and p.get("by") == "gmods-builder" for p in listed), "the lead sees the proposal")
    summary["proposal"] = {"id": pid, "refused": r.get("error")}
    b.close()


def probes(mcp, summary):
    """logic_build refuses modules that reach the OS around the engine."""
    sdk = (ROOT / "crates" / "specula-sdk").as_posix()
    plat = (ROOT / "crates" / "specula-platform").as_posix()
    cases = [
        ("native_code", f'specula-sdk = {{ path = "{sdk}" }}',
         '#![deny(unsafe_code)]\n#[link(name = "user32", kind = "raw-dylib")]\nextern "system" { fn GetAsyncKeyState(key: i32) -> i16; }\n'
         '#[derive(Default)] pub struct K;\nimpl specula_sdk::Module for K { fn describe(&self) -> specula_sdk::ModuleInfo { specula_sdk::ModuleInfo { name: "probe", ..Default::default() } } }\n'
         'specula_sdk::export_module!(K);\n'),
        ("os_crate", f'specula-sdk = {{ path = "{sdk}" }}\nspecula-platform = {{ path = "{plat}" }}',
         '#![deny(unsafe_code)]\n#[derive(Default)] pub struct K;\nimpl specula_sdk::Module for K { fn describe(&self) -> specula_sdk::ModuleInfo { specula_sdk::ModuleInfo { name: "probe", ..Default::default() } } }\n'
         'specula_sdk::export_module!(K);\n'),
    ]
    for code, deps, src in cases:
        shutil.rmtree(PROBE, ignore_errors=True)
        (PROBE / "src").mkdir(parents=True)
        (PROBE / "Cargo.toml").write_text(f'[package]\nname = "gmods_probe"\nversion = "0.1.0"\nedition = "2021"\npublish = false\n\n[lib]\ncrate-type = ["cdylib"]\n\n[dependencies]\n{deps}\n\n[workspace]\n')
        (PROBE / "src" / "lib.rs").write_text(src)
        r, err = build(mcp, PROBE.relative_to(ROOT).as_posix())
        got = (r.get("error") or {}).get("code") if isinstance(r.get("error"), dict) else r.get("error")
        text = json.dumps(r)
        check(err or not r.get("ok"), f"probe {code}: refused")
        check(code in text, f"probe {code}: refused as {code} ({str(got)[:160]})")
        summary.setdefault("probes", {})[code] = str(got)[:300]
    shutil.rmtree(PROBE, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7640)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--window", action="store_true", help="run A in a window (captures for review)")
    ap.add_argument("--adapter", default=None, help="discrete | integrated | <index>")
    ap.add_argument("--out", default=str(target / "gmods-check"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    summary = {"modules": MODULES, "seed": SEED}
    # Only the owner or a granted lead builds and loads modules (docs/design/MODULES.md).
    common = ["--paused", "--audio", "null", "--saves", str(out / "saves"), "--lead", "gmods-player", "--lead", "gmods-twin"]
    shutil.rmtree(out / "saves", ignore_errors=True)
    args_a = common + ["--captures", str(out / "captures-a")]
    if not a.window:
        args_a.append("--headless")
    if a.adapter:
        args_a += ["--adapter", a.adapter]
    args_b = common + ["--headless", "--captures", str(out / "captures-b")]

    print(f"run A ({'window' if a.window else 'headless'}), port {a.port}")
    try:
        with Engine(a.exe, a.port, out / "engine-a.log", *args_a, cwd=str(ROOT)) as engine:
            mcp = engine.mcp("gmods-player")
            mcp.hello("gmods-player", role="lead")
            proposal_probe(engine, mcp, summary)
            build_all(mcp, "A", summary)
            p = Player(mcp, "A")
            p.step(2)
            p.mark("start")
            frames = []

            def frame(name, eye, target):
                f = mcp.frame(width=960, height=540, eye=eye, target=target, overlays=["labels"] if name == "done" else [])
                packet = out / f"frame_{name}.json"
                packet.write_text(json.dumps(f, indent=1))
                frames.append({"name": name, "image": (f.get("image") or {}).get("path"), "packet": str(packet)})

            mid = p.first_half(frame if a.window else None)
            mcp.json("save_write", name="gmods_mid", label="gmods_check: three bones, wisps left")
            end = p.second_half(frame if a.window else None)
            v = mcp.json("world_hash", verify=True)
            check((v.get("verify") or {}).get("matches") is True, f"[A] the journal replays to the live hash ({v.get('verify')})")
            st = mcp.json("logic_status")
            running = [m["name"] for m in st["modules"] if m["status"] == "running"]
            check(sorted(running) == sorted(MODULES), f"[A] all six modules still running ({running})")
            summary["tick_us"] = {m["name"]: m.get("tick_us") for m in st["modules"]}
            played = mcp.json("audio_played", seconds=600, limit=64)
            by = sorted({x.get("by") for x in played.get("plays", []) if str(x.get("by", "")).startswith("module:")})
            names = sorted({x.get("name") for x in played.get("plays", []) if str(x.get("by", "")).startswith("module:")})
            summary["sounds"] = {"by": by, "names": names, "engine": mcp.json("engine_info").get("module_sounds")}
            check({"module:inventarium", "module:facultas", "module:colloquium", "module:quaestio"} <= set(by), f"[A] every gameplay module's sounds played ({by}, {names})")
            # Save mid-quest, load, play the second half again: same end.
            loaded = mcp.json("save_read", name="gmods_mid", force=True)
            summary["save_read"] = {k: loaded.get(k) for k in ("tick", "notes", "modules", "warnings") if k in loaded}
            h = mcp.json("world_hash")
            check(h["state_hash"] == mid["state_hash"], f"[A] the save loads to the saved state ({h['state_hash']} vs {mid['state_hash']})")
            again = Player(mcp, "A-load").second_half()
            check(again["state_hash"] == end["state_hash"], f"[A] after a load the quest ends the same ({again['state_hash']} vs {end['state_hash']})")
            probes(mcp, summary)
            summary["run_a"] = p.marks
            summary["captures"] = frames
            mcp.close()
        check(engine.exit_code == 0 or engine.validation_errors() == 0, f"[A] clean shutdown (exit {engine.exit_code})")

        print(f"run B (headless), port {a.port + 1}")
        with Engine(a.exe, a.port + 1, out / "engine-b.log", *args_b, cwd=str(ROOT)) as engine:
            mcp = engine.mcp("gmods-twin")
            mcp.hello("gmods-twin", role="lead")
            build_all(mcp, "B", summary)
            q = Player(mcp, "B")
            q.step(2)
            q.mark("start")
            q.first_half()
            q.second_half()
            summary["run_b"] = q.marks
            mcp.close()
        for x, y in zip(summary["run_a"], summary["run_b"]):
            check(x == y, f"two runs agree at {x['at']} (tick {x['tick']}: {x['hash']} / {y['hash']})")
        check(len(summary["run_a"]) == len(summary["run_b"]), "both runs reached every checkpoint")
    finally:
        shutil.rmtree(PROBE, ignore_errors=True)
        (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"gmods_check: passed; summary in {out / 'summary.json'}")


if __name__ == "__main__":
    t0 = time.perf_counter()
    main()
    print(f"  took {time.perf_counter() - t0:.0f} s")
