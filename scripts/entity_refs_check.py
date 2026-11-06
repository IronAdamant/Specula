#!/usr/bin/env python3
"""References to entities by identity, end to end (docs/design/EDITOR.md, "Stable entity identity").

Review R3 of the editor lane, as a permanent live check. Notes, critiques and task-board entities
hold an entity's identity (the lineage it was born in and its id), never a path followed through
the journal. It checks, failing loudly:

- race (R3 major 1, windowed, two agents): agent A's world_batch renaming /aK -> /bK and
  /cK -> /aK while agent B notes and critiques /cK/x in the same serve window; every reference
  stays on its entity, also after save_write + save_read (was 16/30 notes, 5/30 critiques wrong);
- follow (R3 minor 1 and probe_follow): chains of renames, undo, reparenting, a batch, delete then
  respawn at the same path (detached, never the newcomer's), rename then delete; save read;
- probe 8 (review R2): a note on /gate renamed /door stays on /door's entity across a save read;
- tput (R3 major 2, headless): with 8192 notes and 8192 critiques on entities, world_set and a
  call right after a 1000-rename world_batch cost what they cost with empty stores, and idle
  frames allocate what they do in a control engine with the same traffic and store sizes on
  paths, without identities (perf_memory: the release build's counting allocator);
- window (R3 major 2, windowed): six 1000-rename batches with full stores make no engine stall.

    cargo build -p specula --release
    python scripts/entity_refs_check.py [--adapter integrated] [--port 7560] [--out DIR] [--exe PATH]

Keep output ASCII.
"""
import argparse
import json
import shutil
import sys
import threading
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

FAILS = []
N = 8192


def check(ok, what):
    print(("PASS " if ok else "FAIL ") + what, flush=True)
    if not ok:
        FAILS.append(what)
    return ok


def raw(m, tool, **args):
    r = m.request("tools/call", {"name": tool, "arguments": args})
    text = next((c["text"] for c in r.get("content", []) if c.get("type") == "text"), "")
    try:
        j = json.loads(text)
    except ValueError:
        j = {"text": text}
    return (text if r.get("isError") else None), j


def ok(m, tool, **args):
    err, j = raw(m, tool, **args)
    if err:
        raise RuntimeError(f"{tool} failed: {err[:300]}")
    return j


def timed(m, tool, **args):
    t = time.perf_counter()
    raw(m, tool, **args)
    return (time.perf_counter() - t) * 1000


def median(xs):
    xs = sorted(xs)
    return xs[len(xs) // 2]


def paged(m, tool, key, **args):
    out, cur = [], None
    while True:
        a = dict(args, limit=200)
        if cur:
            a["cursor"] = cur
        j = ok(m, tool, **a)
        out += j.get(key, [])
        cur = j.get("next_cursor")
        if not cur or not j.get(key):
            return out


def notes(m):
    return {n["text"]: n.get("entity") for n in paged(m, "note_list", "notes")}


def critiques(m):
    return {c["text"]: c.get("entity") for c in paged(m, "critique_list", "critiques", response_format="detailed")}


def on(i, p):
    return {"id": i, "path": p}


def det(p):
    return {"path": p, "detached": True}


def save_and_read(m, name):
    ok(m, "save_write", name=name)
    deadline = time.time() + 30
    while not any(s.get("name") == name for s in ok(m, "save_list").get("saves", [])):
        if time.time() > deadline:
            raise RuntimeError("save_write never landed")
        time.sleep(0.2)
    deadline = time.time() + 60
    while True:
        err, j = raw(m, "save_read", name=name, force=True)
        if not err:
            break
        if "busy" not in err and "retry" not in err or time.time() > deadline:
            raise RuntimeError(f"save_read: {err[:300]}")
        time.sleep(0.3)
    time.sleep(0.5)


def spawner(m):
    return lambda name, parent=None: ok(m, "world_spawn", name=name, shape="cube", **({"parent": parent} if parent else {}))["id"]


def stalls(m):
    j = ok(m, "trace_query", code="engine.stall", limit=50)
    return j.get("matched", len(j.get("events", [])))


def race(eng):
    a = eng.mcp("a"); a.hello("own", role="owner")
    b = eng.mcp("b"); b.hello("other")
    sp = spawner(a)
    pairs = []
    for k in range(30):
        A = sp(f"a{k}"); AX = sp("x", f"/a{k}")
        C = sp(f"c{k}"); CX = sp("x", f"/c{k}")
        res = {}

        def batch():
            res["b"] = raw(a, "world_batch", ops=[{"op": "set", "id": A, "rename": f"b{k}"}, {"op": "set", "id": C, "rename": f"a{k}"}])

        def note():
            res["n"] = raw(b, "note_add", id=CX, text=f"race{k}")
            res["c"] = raw(b, "critique_add", items=[{"text": f"race{k}", "severity": "note", "source": "probe", "entity": CX}])

        t1 = threading.Thread(target=batch); t2 = threading.Thread(target=note)
        t1.start(); time.sleep(0.0005 * (k % 4)); t2.start(); t1.join(); t2.join()
        if not check(not res["b"][0] and not res["n"][0] and not res["c"][0], f"race{k}: the batch, note and critique went in"):
            return
        pairs.append((k, CX, AX))
    n, c = notes(a), critiques(a)
    bad = [k for k, CX, _ in pairs if n[f"race{k}"] != on(CX, f"/a{k}/x") or c[f"race{k}"] != on(CX, f"/a{k}/x")]
    check(not bad, f"race: every note and critique is on its entity at /aK/x (wrong: {bad})")
    save_and_read(a, "race")
    n, c = notes(a), critiques(a)
    bad = [k for k, CX, _ in pairs if n[f"race{k}"] != on(CX, f"/a{k}/x") or c[f"race{k}"] != on(CX, f"/a{k}/x")]
    check(not bad, f"race: after save_read, still on the noted entities, never the other x (wrong: {bad})")


def follow(eng):
    m = eng.mcp("f"); m.hello("own", role="owner")
    sp = spawner(m)
    P = sp("p"); PC = sp("kid", "/p")
    ok(m, "note_add", id=PC, text="chain")
    for nm in ["q", "r", "s"]:
        ok(m, "world_set", id=P, rename=nm)
    check(notes(m)["chain"] == on(PC, "/s/kid"), "follow: a chain of parent renames")
    U = sp("u"); UC = sp("kid", "/u")
    ok(m, "note_add", id=UC, text="undo")
    ok(m, "world_set", id=U, rename="v")
    seq = ok(m, "world_history", limit=1)["entries"][0]["id"]
    ok(m, "world_undo", ids=[seq])
    check(notes(m)["undo"] == on(UC, "/u/kid"), "follow: an undone rename")
    sp("s1"); sp("s2"); K = sp("k", "/s1"); KK = sp("kk", "/s1/k")
    ok(m, "note_add", id=KK, text="reparent")
    ok(m, "world_set", id=K, parent="/s2")
    check(notes(m)["reparent"] == on(KK, "/s2/k/kk"), "follow: reparented")
    B = sp("bp"); BC = sp("bc", "/bp")
    ok(m, "note_add", id=BC, text="batch")
    ok(m, "world_batch", ops=[{"op": "set", "id": B, "rename": "bp2"}, {"op": "set", "id": BC, "rename": "bc2"}, {"op": "set", "id": B, "rename": "bp3"}])
    check(notes(m)["batch"] == on(BC, "/bp3/bc2"), "follow: one batch renaming parent, child, parent")
    G = sp("gate")
    ok(m, "note_add", id=G, text="gate")
    ok(m, "critique_add", items=[{"text": "gate", "severity": "note", "source": "probe", "entity": G}])
    ok(m, "world_delete", id=G)
    G2 = sp("gate")
    check(notes(m)["gate"] == det("/gate") and critiques(m)["gate"] == det("/gate"), f"follow: deleted -> detached, never on the new /gate ({G2})")
    D = sp("dd")
    ok(m, "note_add", id=D, text="dd")
    ok(m, "world_set", id=D, rename="ee")
    check(notes(m)["dd"] == on(D, "/ee"), "follow: renamed")
    ok(m, "world_delete", id=D)
    sp("ee")
    check(notes(m)["dd"] == det("/ee"), "follow: renamed then deleted -> detached at its last path")
    # Review R2's probe 8.
    g = sp("gate8")
    ok(m, "note_add", id=g, text="hinges are placeholders")
    ok(m, "world_set", id=g, rename="door8")
    g2 = sp("gate8")
    save_and_read(m, "follow")
    n = notes(m)
    check(n["chain"] == on(PC, "/s/kid") and n["undo"] == on(UC, "/u/kid") and n["reparent"] == on(KK, "/s2/k/kk") and n["batch"] == on(BC, "/bp3/bc2"),
          "follow: after save_read every attached note is on its entity")
    check(n["gate"] == det("/gate") and n["dd"] == det("/ee"), "follow: after save_read the detached ones stay detached")
    check(n["hinges are placeholders"] == on(g, "/door8"), f"probe 8: on /door8's entity after save_read, never the new /gate8 ({g2})")
    total = ok(m, "note_list", entity=g2, limit=1)["total"]
    check(total == 0, "probe 8: nothing on the new /gate8")


def fill(eng, own, n_threads=8):
    for b in range(0, N, 1000):
        ok(own, "world_batch", ops=[{"op": "spawn", "shape": "group", "path": f"/r{i}"} for i in range(b, min(N, b + 1000))])
    ids = {e["path"]: e["id"] for e in paged(own, "world_list", "entities")}
    return ids


def add_refs(eng, own, ids, on_paths=False):
    """8192 notes (8 agents) and 8192 critiques: on the entities (identities), or with
    `on_paths` on their paths (no identities: the control, the same traffic and store sizes)."""
    def note_args(i):
        return {"on_path": f"/r{i}"} if on_paths else {"id": ids[f"/r{i}"]}

    def crit_target(i):
        return {"path": f"/r{i}"} if on_paths else {"entity": ids[f"/r{i}"]}

    def add(lo, hi, m):
        for i in range(lo, hi):
            raw(m, "note_add", text=f"n{i}", **note_args(i))
    ths = []
    for t in range(8):
        m = eng.mcp(f"w{t}"); m.hello(f"w{t}")
        th = threading.Thread(target=add, args=(t * N // 8, (t + 1) * N // 8, m)); th.start(); ths.append(th)
    for th in ths:
        th.join()
    for b in range(0, N, 32):
        ok(own, "critique_add", items=[dict(text=f"c{i}", severity="note", source="probe", **crit_target(i)) for i in range(b, min(N, b + 32))])
    nt, ct = ok(own, "note_list", limit=1)["total"], ok(own, "critique_list", limit=1)["total"]
    check(nt >= N and ct >= N, f"{nt} notes and {ct} critiques {'on paths' if on_paths else 'on entities'}")


def tput_run(eng, on_paths):
    """Fills one engine's world (8192 groups), measures with empty stores, then with 8192 notes
    and 8192 critiques (on entities, or on paths for the control)."""
    own = eng.mcp("t"); own.hello("tput")
    ids = fill(eng, own)
    spot = ok(own, "world_spawn", name="spot", shape="cube")["id"]

    def measure(label):
        # perf_memory's history is the last 120 frames: about 100 ms each headless, so after
        # 14 s of nothing they are all idle frames.
        time.sleep(14.0)
        mem = ok(own, "perf_memory").get("per_frame", {}).get("allocations") or {}
        sets = [timed(own, "world_set", id=spot, position=[k * 0.01, 0, 0]) for k in range(200)]
        lat = []
        for rnd in range(4):
            a, b = ("r", "z") if rnd % 2 == 0 else ("z", "r")
            ok(own, "world_batch", ops=[{"op": "set", "path": f"/{a}{i}", "rename": f"{b}{i}"} for i in range(1000)])
            lat.append(timed(own, "claim_list"))
        r = {"idle_allocs_mean": mem.get("mean"), "idle_allocs_max": mem.get("max"), "set_ms": median(sets), "after_batch_ms": max(lat)}
        print(f"  {label}: {json.dumps(r)}")
        return r

    empty = measure("empty stores")
    add_refs(eng, own, ids, on_paths)
    full = measure("8192 notes + 8192 critiques " + ("on paths (control)" if on_paths else "on entities"))
    t = time.perf_counter()
    page = ok(own, "note_list", limit=50)
    full["note_list_ms"] = (time.perf_counter() - t) * 1000
    full["notes"] = page["total"]
    return empty, full


def tput(exe, port, out, common):
    """R3 major 2 headless: what references on entities cost, against empty stores and against
    the same traffic and store sizes without identities (notes and critiques on paths)."""
    with Engine(exe, port, out / "tput_control.log", "--headless", *common, cwd=str(out)) as eng:
        _, control = tput_run(eng, True)
    with Engine(exe, port, out / "tput.log", "--headless", *common, cwd=str(out)) as eng:
        empty, full = tput_run(eng, False)
    (out / "tput.json").write_text(json.dumps({"empty": empty, "full": full, "control": control}, indent=1))
    check(full["after_batch_ms"] <= empty["after_batch_ms"] + 10 and full["after_batch_ms"] < 30,
          f"tput: a call right after a 1000-rename batch: {full['after_batch_ms']:.1f} ms full vs {empty['after_batch_ms']:.1f} ms empty (R3: 83-108 ms)")
    check(full["set_ms"] <= empty["set_ms"] + 2, f"tput: world_set median {full['set_ms']:.2f} ms full vs {empty['set_ms']:.2f} ms empty")
    if check(full["idle_allocs_mean"] is not None and control["idle_allocs_mean"] is not None,
             "tput: perf_memory counts allocations (the release build's counting allocator)"):
        check(full["idle_allocs_mean"] <= control["idle_allocs_mean"] + 2.0,
              f"tput: idle frames allocate as with no identities (mean {full['idle_allocs_mean']} on entities vs {control['idle_allocs_mean']} "
              f"on paths, {empty['idle_allocs_mean']} empty; max {full['idle_allocs_max']} vs {control['idle_allocs_max']})")
    check(full["notes"] >= N and full["note_list_ms"] < 20, f"tput: note_list over {full['notes']} notes in {full['note_list_ms']:.1f} ms")


def window(eng, out):
    own = eng.mcp("win"); own.hello("own", role="owner")
    ids = fill(eng, own)
    add_refs(eng, own, ids)
    time.sleep(2.0)
    s0 = stalls(own)
    t = time.perf_counter()
    for rnd in range(6):
        a, b = ("r", "z") if rnd % 2 == 0 else ("z", "r")
        ok(own, "world_batch", ops=[{"op": "set", "path": f"/{a}{i}", "rename": f"{b}{i}"} for i in range(1000)])
    took = (time.perf_counter() - t) * 1000
    time.sleep(1.0)
    s1 = stalls(own)
    pf = ok(own, "perf_frame", response_format="detailed")
    (out / "window_perf_frame.json").write_text(json.dumps(pf, indent=1))
    check(s1 == s0, f"window: six 1000-rename batches with full stores in {took:.0f} ms, no engine stall ({s1 - s0} new; R3: 725 ms and a 131 ms frame)")
    n = ok(own, "note_list", entity=ids["/r5"], limit=1)
    check(n["total"] == 1 and n["notes"][0]["entity"] == on(ids["/r5"], "/r5"), "window: the notes still name their entities")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7560)
    ap.add_argument("--out", default=str(ROOT / "target" / "entity-refs-check"))
    ap.add_argument("--exe", default=str(ROOT / "target" / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    # This run's own saves and worlds only (an earlier run's would belong to other sessions).
    for d in ("saves", "worlds"):
        shutil.rmtree(out / d, ignore_errors=True)
    common = ["--owner", "own", "--captures", str(out / "cap"), "--saves", str(out / "saves"), "--worlds", str(out / "worlds")]
    print("-- headless: follow, probe 8")
    with Engine(a.exe, a.port, out / "follow.log", "--headless", *common, cwd=str(out)) as eng:
        follow(eng)
    print("-- headless: tput (a control engine with notes and critiques on paths, then on entities)")
    tput(a.exe, a.port, out, common)
    print("-- windowed: race")
    with Engine(a.exe, a.port + 1, out / "race.log", "--adapter", a.adapter, "--size", "640x400", *common, cwd=str(out)) as eng:
        race(eng)
    print("-- windowed: window cost")
    with Engine(a.exe, a.port + 1, out / "window.log", "--adapter", a.adapter, "--size", "640x400", *common, cwd=str(out)) as eng:
        window(eng, out)
    print(f"{len(FAILS)} failure(s)" if FAILS else "entity_refs_check: PASS")
    sys.exit(1 if FAILS else 0)


if __name__ == "__main__":
    main()
