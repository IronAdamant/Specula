#!/usr/bin/env python3
"""A playtest swarm: many headless engines in parallel, one playtest each, one report.

A lead engine (started here, headless, on --lead-port; or an engine already listening there with
--use-lead) coordinates:

- the **task board**: one task per playtest (`task_post`, tag `playtest`). Each worker (a thread
  with its own MCP session, `agent_hello` as `playtester-N`) takes a ready task (`task_list
  {ready_for_me}`, `task_take`), runs it and finishes it with a summary (`task_done`);
- **resource leases**: each worker leases two ports (`lease_take {kind: port, count: 2}`, from the
  lead's pool, 7500-7599 by default) and a scratch folder (`lease_take {kind: scratch}`).

Per playtest a worker starts a **fresh** headless engine (`--headless --paused`, working directory
in its scratch folder) on one of its ports, runs the spec with `playtest_run {record}`, stops the
engine, then starts a second fresh engine on its other port and runs the replay file
(`playtest_run {replay}`), so every replay is verified by hash in a separate process. Engines are
stopped by PID (Ctrl+Break, then kill), also on Ctrl+C or an error: every live engine is tracked. At the end the workers release their leases (scratch folders
deleted unless --keep-scratch), and the report (every playtest's status, failures, ticks, wall time,
verification) is written to --out. Replays and captures are copied next to it.

    python scripts/playtest_swarm.py --exe path/to/sepulcra.exe --specs specs_dir/ --workers 8
    python scripts/playtest_suite.py        # Sepulcra's suite (uses this module)

Bounded: at most --workers engines at once (default 4, max 16), each playtest's own limits
(max_wall_s), captures copied at most 8 per playtest. Standard library only; output ASCII.
"""
import argparse
import contextlib
import json
import shutil
import sys
import threading
import time
from pathlib import Path

from specula_mcp import ROOT, Engine, Mcp, server_up, url_for

MAX_WORKERS = 16
MAX_COPIED_CAPTURES = 8


def finish(mcp, first):
    """Polls playtest_report until the playtest that `first` answered for has ended."""
    r = first
    while r.get("status") == "running":
        r = mcp.json("playtest_report", id=r["id"], wait_s=20)
    return r


def short(report):
    """What the swarm report keeps of an engine's playtest report."""
    keep = ("status", "ticks", "until_met_at", "failures_total", "failures", "wall_ms", "ticks_per_s", "steps", "reason")
    out = {k: report[k] for k in keep if k in report}
    if "hash" in report:
        out["end_hash"] = report["hash"].get("end")
    if "replay" in report:
        r = report["replay"]
        out["replay"] = {k: r.get(k) for k in ("verified", "checked", "expected", "start_matches", "first_divergence") if k in r}
    return out


class Swarm:
    # The board's tag and title word for this swarm's tasks (the evals runner uses "eval").
    task_tag = "playtest"
    task_word = "playtest"

    def __init__(self, exe, out, workers=4, lead_port=7493, use_lead=False, engine_args=(), game_args=(), verify=True,
                 keep_scratch=False, log=print, session_format=None, ports_per_worker=None):
        self.exe, self.out = Path(exe), Path(out)
        self.workers = max(1, min(int(workers), MAX_WORKERS))
        self.lead_port, self.use_lead = lead_port, use_lead
        self.engine_args, self.game_args = list(engine_args), list(game_args)
        self.verify, self.keep_scratch, self.log = verify, keep_scratch, log
        # Each worker leases a port per engine it runs at once: two (record, then verify), or one
        # without verification (more workers from a small port pool).
        self.ports_per_worker = ports_per_worker or (2 if verify else 1)
        # The playtesting session's default response_format (agent_hello), when set.
        self.session_format = session_format
        self.lock = threading.Lock()
        self.results = {}
        self.errors = []
        # Set on Ctrl+C or an error: workers take no more tasks.
        self.stopping = threading.Event()
        # Engines running now (stopped by PID in run()'s finally, whatever happened).
        self.live = set()

    @contextlib.contextmanager
    def started(self, engine):
        """Runs `engine`, tracked from before it starts until it has stopped, so stop_all() also
        reaches an engine that is still starting. Refused once the swarm is stopping."""
        if self.stopping.is_set():
            raise RuntimeError("the swarm is stopping: no new engines")
        with self.lock:
            self.live.add(engine)
        try:
            with engine as eng:
                # stop_all() may have run while it started (before its process existed).
                if self.stopping.is_set():
                    raise RuntimeError("the swarm is stopping")
                yield eng
        finally:
            with self.lock:
                self.live.discard(engine)

    def stop_all(self):
        """Stops every tracked engine (by PID: Ctrl+Break, then kill)."""
        self.stopping.set()
        with self.lock:
            engines = list(self.live)
        # All at once: each stop waits for its engine's clean exit.
        stoppers = [threading.Thread(target=eng.stop, daemon=True) for eng in engines]
        for t in stoppers:
            t.start()
        for t in stoppers:
            t.join(40)

    def engine(self, port, cwd, label, cache):
        cwd.mkdir(parents=True, exist_ok=True)
        # The worker's own pipeline cache: warm from its second engine on, and never written by two
        # engines at once.
        args = ["--headless", "--paused", "--captures", str(cwd / "captures"), "--cache", str(cache), *self.engine_args]
        if self.game_args:
            args += ["--", *self.game_args]
        return Engine(self.exe, port, cwd / f"{label}.log", *args, cwd=cwd)

    def session(self, eng, worker):
        """The worker's MCP session on one of its engines (with its default format, if set). Its
        own calls get the engine's answers as an agent would (concise unless the default says
        otherwise); the report's fields read here are in both formats."""
        mcp = eng.mcp(f"playtester-{worker}", detailed=False)
        if self.session_format:
            mcp.json("agent_hello", name=f"playtester-{worker}", role="playtester", response_format=self.session_format)
        return mcp

    def run_one(self, worker, name, spec, ports, scratch):
        """Records the spec in one fresh engine, then verifies the replay in another."""
        job = scratch / name
        result = {"name": name, "worker": worker}
        start = time.perf_counter()
        rec_dir = job / "record"
        with self.started(self.engine(ports[0], rec_dir, "engine", scratch / "pipeline-cache")) as eng:
            result["startup_s"] = round(eng.startup_s, 2)
            mcp = self.session(eng, worker)
            r = finish(mcp, mcp.json("playtest_run", spec=spec, record="replay.json", overwrite=True))
            mcp.close()
        result["run"] = short(r)
        # record writes under the engine's playtests/ folder.
        replay_src = rec_dir / "playtests" / "replay.json"
        if replay_src.exists():
            (self.out / "replays").mkdir(parents=True, exist_ok=True)
            shutil.copyfile(replay_src, self.out / "replays" / f"{name}.json")
        caps = r.get("captures", [])[:MAX_COPIED_CAPTURES]
        copied = []
        for c in caps:
            png = Path(c.get("png") or "")
            if png.exists():
                dest = self.out / "captures" / name
                dest.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(png, dest / f"{c['label'].replace('@', '_')}.png")
                packet = png.with_suffix(".json")
                if packet.exists():
                    shutil.copyfile(packet, dest / f"{c['label'].replace('@', '_')}.json")
                copied.append(str((dest / f"{c['label'].replace('@', '_')}.png").relative_to(self.out)))
        if copied:
            result["captures"] = copied
        if self.stopping.is_set():
            raise RuntimeError("the swarm is stopping: the replay was not checked")
        if self.verify and replay_src.exists():
            ver_dir = job / "verify"
            ver_dir.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(replay_src, ver_dir / "replay.json")
            with self.started(self.engine(ports[-1], ver_dir, "engine", scratch / "pipeline-cache")) as eng:
                mcp = self.session(eng, worker)
                v = finish(mcp, mcp.json("playtest_run", replay="replay.json"))
                mcp.close()
                result["verify_startup_s"] = round(eng.startup_s, 2)
            result["verify"] = short(v)
        result["wall_s"] = round(time.perf_counter() - start, 2)
        ok_run = r.get("status") == "passed"
        ok_ver = not self.verify or (result.get("verify", {}).get("replay", {}).get("verified") is True
                                      and result["verify"].get("status") == r.get("status"))
        result["passed"] = bool(ok_run and ok_ver)
        return result

    def summary(self, res):
        """The one line a worker reports on the board for a finished job."""
        status = res.get("run", {}).get("status", "error")
        verified = res.get("verify", {}).get("replay", {}).get("verified")
        return f"{status}; replay {'verified' if verified else 'NOT verified'}; {res.get('wall_s', 0)} s"

    def worker(self, k, lead_url):
        """One worker, start to end. What stops it (a lease the pool cannot give, an engine
        call that fails) is kept: a worker that dies leaves the others to do its share, and the
        run would read as whole (M5's audit: three of six workers without ports, the suite
        "passed with 6 workers")."""
        try:
            self.work(k, lead_url)
        except Exception as e:  # noqa: BLE001 (whatever it was, the run says so)
            with self.lock:
                self.errors.append(f"playtester-{k}: {str(e)[:400]}")

    def work(self, k, lead_url):
        name = f"playtester-{k}"
        with Mcp(lead_url, name) as lead:
            lead.json("agent_hello", name=name, role="playtester")
            try:
                # Inside the try: whatever was leased is released, even if the second lease fails.
                ports = lead.json("lease_take", kind="port", count=self.ports_per_worker, note="playtest engines", lease_s=3600)
                scratch = lead.json("lease_take", kind="scratch", note="playtest working folders", lease_s=3600)
                port_list, folder = ports["ports"], Path(scratch["dir"])
                self.log(f"  {name}: ports {port_list}, scratch {folder}")
                while not self.stopping.is_set():
                    ready = lead.json("task_list", ready_for_me=True, tag=self.task_tag, limit=20).get("tasks", [])
                    if not ready:
                        break
                    took = None
                    for t in ready:
                        try:
                            lead.json("task_take", id=t["id"])
                            took = t
                            break
                        except RuntimeError as e:
                            if "task_taken" in str(e) or "task_finished" in str(e):
                                continue
                            raise
                    if took is None:
                        continue
                    pt_name = took["title"].split(" ", 1)[1]
                    spec = self.specs[pt_name]
                    try:
                        res = self.run_one(k, pt_name, spec, port_list, folder)
                    except Exception as e:  # an engine that wouldn't start: the playtest failed
                        res = {"name": pt_name, "worker": k, "passed": False, "error": str(e)[:500]}
                    with self.lock:
                        self.results[pt_name] = res
                    summary = self.summary(res)
                    lead.json("task_done", id=took["id"], summary=summary[:200])
                    self.log(f"  {name}: {pt_name}: {'PASS' if res['passed'] else 'FAIL'} ({summary})")
            finally:
                lead.json("lease_release", all=True, **({"keep": True} if self.keep_scratch else {"delete": True}))

    def run(self, specs):
        """specs: {name: spec}. Returns the report (also written to out/report.json)."""
        self.specs = specs
        self.out.mkdir(parents=True, exist_ok=True)
        started = time.perf_counter()
        lead_engine = None
        if not self.use_lead:
            lead_dir = self.out / "lead"
            lead_dir.mkdir(parents=True, exist_ok=True)
            # (The lead of the swarm is named by its operator, this script: --lead.)
            args = ["--headless", "--paused", "--lead", "playtest-lead", "--captures", str(lead_dir / "captures"), *self.engine_args]
            if self.game_args:
                args += ["--", *self.game_args]
            lead_engine = Engine(self.exe, self.lead_port, lead_dir / "lead.log", *args, cwd=lead_dir).__enter__()
        elif not server_up(self.lead_port):
            raise SystemExit(f"--use-lead: nothing listens on port {self.lead_port}")
        lead_url = url_for(self.lead_port)
        try:
            with Mcp(lead_url, "playtest-lead") as lead:
                lead.json("agent_hello", name="playtest-lead", role="lead")
                task_ids = []
                for name, spec in specs.items():
                    t = lead.json("task_post", title=f"{self.task_word} {name}", tags=[self.task_tag],
                                  detail=(spec.get("description") or spec.get("summary") or f"run {self.task_word} {name}")[:4000])
                    task_ids.append(t["id"])
                self.log(f"posted {len(task_ids)} {self.task_word} tasks; {self.workers} workers")
                threads = [threading.Thread(target=self.worker, args=(k + 1, lead_url), daemon=True) for k in range(self.workers)]
                for th in threads:
                    th.start()
                try:
                    # Short joins, so Ctrl+C reaches this thread.
                    while any(th.is_alive() for th in threads):
                        for th in threads:
                            th.join(0.5)
                except KeyboardInterrupt:
                    self.log("interrupted: stopping the engines and releasing the leases")
                    self.stop_all()
                    for th in threads:
                        th.join(60)
                    raise
                board = lead.json("task_list", tag=self.task_tag, limit=200)
                done = sum(1 for t in board.get("tasks", []) if t.get("status") == "done")
                if self.errors:
                    raise RuntimeError(f"{len(self.errors)} of {self.workers} workers failed: " + "; ".join(self.errors))
        finally:
            self.stop_all()
            if lead_engine is not None:
                lead_engine.__exit__(None, None, None)
        wall = time.perf_counter() - started
        results = [self.results.get(n, {"name": n, "passed": False, "error": "not run"}) for n in specs]
        report = {
            "workers": self.workers,
            "playtests": len(results),
            "passed": sum(1 for r in results if r.get("passed")),
            "failed": [r["name"] for r in results if not r.get("passed")],
            "verified": sum(1 for r in results if r.get("verify", {}).get("replay", {}).get("verified")),
            "wall_s": round(wall, 2),
            "sum_of_playtest_wall_s": round(sum(r.get("wall_s", 0) for r in results), 2),
            "board": {"tasks_done": done, "tasks": len(task_ids)},
            "results": results,
        }
        (self.out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
        return report


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", required=True, help="engine binary (a game's, e.g. sepulcra.exe, or specula.exe)")
    ap.add_argument("--specs", required=True, help="folder of playtest spec files (*.json)")
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--lead-port", type=int, default=7493)
    ap.add_argument("--use-lead", action="store_true", help="use the engine already on --lead-port as the lead")
    ap.add_argument("--out", default=str(ROOT / "target" / "playtest-swarm"))
    ap.add_argument("--no-verify", action="store_true")
    ap.add_argument("--keep-scratch", action="store_true")
    ap.add_argument("game_args", nargs="*", help="after --: the game's own arguments (e.g. --mute)")
    a = ap.parse_args()
    specs = {}
    for p in sorted(Path(a.specs).glob("*.json")):
        s = json.loads(p.read_text(encoding="utf-8"))
        specs[s["name"]] = s
    swarm = Swarm(a.exe, a.out, a.workers, a.lead_port, a.use_lead, game_args=a.game_args, verify=not a.no_verify,
                  keep_scratch=a.keep_scratch)
    r = swarm.run(specs)
    print(f"{r['passed']}/{r['playtests']} passed, {r['verified']} replays verified, {r['wall_s']} s wall "
          f"({r['sum_of_playtest_wall_s']} s of playtests); report in {a.out}/report.json")
    sys.exit(0 if not r["failed"] else 1)


if __name__ == "__main__":
    main()
