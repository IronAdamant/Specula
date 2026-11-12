"""Live check of localisation on Sepulcra (lane m5-l10n, docs/design/LOCALISATION.md).

On a scratch copy of Sepulcra's assets, with the German table taken out, agents run the
translation flow through the tools alone:

- a translator makes the German table (l10n_set create), the lead posts the flow's tasks
  (l10n_tasks), the translator claims the table and writes every string (l10n_set, 200 at a time),
  and the file it makes is the committed games/sepulcra/assets/strings/de.json byte for byte;
- refusals: a placeholder that isn't the source's, a key the source lacks, status/src given by
  the caller, a translator marking strings reviewed (l10n_set review and project_patch), another
  agent writing the claimed table;
- a reviewer reviews strings; the source changes and the translation goes stale (l10n_get,
  l10n_check), a stale string can't be reviewed, translating it again clears it; world_undo
  takes a translation back;
- the game hot-reloads the tables, switches language from its settings (ui_act on the Language
  button), and every screen passes ui_check in English, German and the pseudo-locale at every
  display and UI-scale setting, in the start and a busy state (l10n_check {ui});
- ui_frame captures of the HUD, the pause menu and the settings in each language.

Usage:
    cargo build --release -p sepulcra-app
    python scripts/l10n_check.py [--port 7620] [--adapter integrated] [--out DIR]
    python scripts/l10n_check.py --make-de ENTRIES.json   (writes de.json through l10n_set)

Fails loudly (exit 1). Writes summary.json and the captures to --out (default target/l10n-check).
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

ASSETS = ROOT / "games" / "sepulcra" / "assets"
DE = ASSETS / "strings" / "de.json"
DE_PATH = "assets/strings/de.json"
EN_PATH = "assets/strings/en.json"
UI_SCALES = [0.8, 0.9, 1.0, 1.15, 1.3, 1.5]
TARGETS = ["deck", "1080p", "4k", "16:10", "ultrawide"]
GROUPS = [["hud"], ["hud", "radial"], ["hud", "pause"], ["hud", "pause", "settings"], ["hud", "pause", "controls"],
          ["hud", "pause", "saves"], ["hud", "pause", "confirm"], ["hud", "victory"], ["hud", "defeat"], ["title"]]
UI_RULES = ["legibility", "contrast", "non_text_contrast", "overlap", "offscreen", "overflow", "text_overflow", "missing_glyph", "touch_target",
            "focus_reachability", "focus_scope", "gamepad_access", "unbound", "unknown_texture", "untranslated", "missing_key", "hardcoded"]

failures = []


def fail(msg):
    # ASCII only (HANDOFF: script output stays ASCII on every console).
    msg = msg.encode("ascii", "backslashreplace").decode()
    print(f"  FAIL: {msg}")
    failures.append(msg)


def ok(msg):
    print(f"  ok: {msg.encode('ascii', 'backslashreplace').decode()}")


def raw(mcp, tool, args):
    r = mcp.request("tools/call", {"name": tool, "arguments": args})
    t = next((c["text"] for c in r["content"] if c["type"] == "text"), "")
    try:
        return bool(r.get("isError")), json.loads(t)
    except json.JSONDecodeError:
        return bool(r.get("isError")), t


def expect_error(mcp, tool, args, code, label):
    err, j = raw(mcp, tool, args)
    got = j.get("error") if isinstance(j, dict) else None
    if not err or got != code:
        fail(f"{label}: expected {code}, got {'error ' + str(got) if err else 'success'}: {str(j)[:300]}")
    else:
        ok(f"{label}: {code}")
    return j


def act(mcp, args):
    for _ in range(50):
        err, j = raw(mcp, "ui_act", args)
        if err or not j.get("rest"):
            return err, j
        args = dict(args, steps=j["rest"])
    fail(f"ui_act never finished: {str(j)[:300]}")
    return True, j


def entries_of(table):
    """A table's entries as l10n_set takes them (text or plural forms, notes kept)."""
    out = {}
    for k, v in table["strings"].items():
        if isinstance(v, str):
            out[k] = v
        else:
            out[k] = {x: y for x, y in v.items() if x not in ("status", "src")}
    return out


def write_all(mcp, locale, entries, create_name=None):
    keys = list(entries)
    for i in range(0, len(keys), 200):
        chunk = {k: entries[k] for k in keys[i:i + 200]}
        args = {"locale": locale, "entries": chunk}
        if create_name and i == 0:
            args["create"] = {"name": create_name}
        err, j = raw(mcp, "l10n_set", args)
        if err:
            fail(f"l10n_set {locale} [{i}..{i + len(chunk)}]: {str(j)[:400]}")
            return None
    return j


def ui_pass(mcp, label, locales):
    """Every screen group in each locale at every display and UI-scale setting. Under 1.0 every
    rule but legibility (the Deck at 0.8 and 0.9 is under its legibility floor: the player's
    choice, an owner question since m5-uitools)."""
    res = {}
    larger = [s for s in UI_SCALES if s >= 1.0]
    smaller = [s for s in UI_SCALES if s < 1.0]
    for scales, rules in [(larger, UI_RULES), (smaller, [r for r in UI_RULES if r != "legibility"])]:
        err, j = raw(mcp, "l10n_check", {"locales": locales, "groups": GROUPS, "targets": TARGETS, "scales": scales, "rules": rules, "max_findings": 40})
        if err:
            fail(f"{label} l10n_check: {str(j)[:500]}")
            continue
        for loc in j["ui"]["locales"]:
            r = res.setdefault(loc["locale"], {"errors": 0, "warnings": 0, "by_rule": {}})
            r["errors"] += loc["errors"]
            r["warnings"] += loc["warnings"]
            for rule, c in loc["by_rule"].items():
                b = r["by_rule"].setdefault(rule, {"errors": 0, "warnings": 0})
                b["errors"] += c["errors"]
                b["warnings"] += c["warnings"]
            for f in loc["findings"]:
                if f["severity"] == "error":
                    fail(f"{label} {loc['locale']} [{f['rule']}] {f.get('group')} {f['target']} {f['screen']}:{f['widget']} {f['message']}")
        tables = j["tables"]
        if tables["errors"]:
            for f in tables["findings"]:
                if f["severity"] == "error":
                    fail(f"{label} tables [{f['rule']}] {f.get('locale')} {f.get('key')} {f['message']}")
    for loc, r in res.items():
        print(f"{label}: {loc}: {r['errors']} errors, {r['warnings']} warnings over {len(GROUPS)} groups x {len(TARGETS)} displays x {len(UI_SCALES)} scales; {r['by_rule']}")
    return res


def capture(mcp, out, name, caps):
    err, j = raw(mcp, "ui_frame", {"display": {"size": [1280, 800]}})
    if err:
        fail(f"ui_frame {name}: {str(j)[:300]}")
        return
    img = j.get("image", {})
    caps.append({"label": name, "path": img.get("path"), "packet": img.get("packet_path")})
    print(f"  capture {name}: {img.get('path')}")


def flow(exe, port, adapter, out, summary, make_de=None):
    work = out / "work"
    if work.exists():
        shutil.rmtree(work)
    assets = work / "assets"
    shutil.copytree(ASSETS, assets)
    committed = DE.read_text(encoding="utf-8") if DE.exists() else None
    de_entries = json.loads(Path(make_de).read_text(encoding="utf-8")) if make_de else entries_of(json.loads(committed))
    (assets / "strings" / "de.json").unlink(missing_ok=True)
    args = ["--headless", "--adapter", adapter, "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(work / "worlds"),
            "--saves", str(work / "saves"), "--lead", "director", "--reviewer", "rev", "--", "--seed", "7", "--mute", "--intro", "off"]
    with Engine(exe, port, out / "engine.log", *args, cwd=str(work)) as eng:
        lead, tr, rev, other = eng.mcp("director"), eng.mcp("translator"), eng.mcp("rev"), eng.mcp("other")
        lead.json("agent_hello", name="director", role="lead")
        tr.json("agent_hello", name="translator", role="translator")
        rev.json("agent_hello", name="rev", role="reviewer")
        other.json("agent_hello", name="other", role="builder")
        lead.call("time_control", action="pause")
        print("tables at start:")
        o = lead.json("l10n_get")
        locs = [t["locale"] for t in o["tables"]]
        if locs != ["en"] or o.get("source") != "en" or o.get("game_locale") != "en":
            fail(f"start: tables {locs}, source {o.get('source')}, game {o.get('game_locale')}")
        else:
            ok(f"one source table en ({o['tables'][0]['keys']} keys), the game shows en")
        # The translator makes the table with its first string; the lead posts the flow.
        first = next(iter(de_entries))
        err, j = raw(tr, "l10n_set", {"locale": "de", "create": {"name": "Deutsch"}, "entries": {first: de_entries[first]}})
        if err:
            fail(f"create de: {str(j)[:300]}")
        err, plan = raw(lead, "l10n_tasks", {"locales": ["de"]})
        if err:
            fail(f"l10n_tasks: {str(plan)[:300]}")
        else:
            ok(f"l10n_tasks posted a plan: {str(plan)[:160]}")
        tasks = lead.json("task_list")
        titles = [t.get("title", "") for t in tasks.get("tasks", [])]
        if not any(t.startswith("Translate") and "(de)" in t for t in titles) or not any(t.startswith("Review") and "(de)" in t for t in titles):
            fail(f"the board lacks the translate/review tasks: {titles}")
        claim = tr.json("claim_take", path=f"/project/{DE_PATH}", lease_s=600)
        # Refusals, before the bulk write.
        expect_error(tr, "l10n_set", {"locale": "de", "entries": {"hud.wave_waves": "Welle {welle}/{waves}"}}, "placeholders", "a placeholder that isn't the source's")
        expect_error(tr, "l10n_set", {"locale": "de", "entries": {"hud.no_such": "x"}}, "unknown_key", "a key the source lacks")
        expect_error(tr, "l10n_set", {"locale": "de", "entries": {"hud.menu": {"text": "Menü", "status": "reviewed"}}}, "invalid_value", "status given by the caller")
        expect_error(other, "l10n_set", {"locale": "de", "entries": {"hud.menu": "Menü"}}, "claimed", "another agent writing the claimed table")
        expect_error(tr, "l10n_set", {"locale": "de", "entries": {first: de_entries[first]}, "review": [first]}, "invalid_argument", "one key written and reviewed in one call")
        # Every string.
        write_all(tr, "de", de_entries)
        made = (assets / "strings" / "de.json").read_text(encoding="utf-8")
        if make_de:
            DE.write_text(made, encoding="utf-8", newline="\n")
            print(f"  wrote {DE} ({len(made)} bytes) from the tools' file")
        elif made.replace("\r\n", "\n") != committed.replace("\r\n", "\n"):
            fail("the table the tools made differs from the committed de.json")
        else:
            ok("the table the tools made is the committed de.json, byte for byte")
        st = tr.json("l10n_get", locale="de", status="missing", limit=5)
        if st["total"]:
            fail(f"{st['total']} German strings still missing: {[e['key'] for e in st['entries']]}")
        else:
            ok("no German string missing")
        # Reviews: the translator may not; the reviewer may.
        expect_error(tr, "l10n_set", {"locale": "de", "review": ["hud.menu"]}, "not_reviewer", "a translator reviewing (l10n_set)")
        expect_error(tr, "project_patch", {"path": DE_PATH, "ops": [{"find": '"hud.menu": {"text": "Menü", "src": "', "replace": '"hud.menu": {"text": "Menü", "status": "reviewed", "src": "'}]}, "not_reviewer", "a translator reviewing (project_patch)")
        tr.json("claim_release", id=claim["id"])
        reviewed = ["hud.menu", "hud.wave_waves", "pause.resume", "settings.language", "language.name"]
        err, j = raw(rev, "l10n_set", {"locale": "de", "review": reviewed})
        if err:
            fail(f"the reviewer's review: {str(j)[:300]}")
        g = rev.json("l10n_get", locale="de", status="reviewed")
        if g["total"] != len(reviewed):
            fail(f"reviewed: {g['total']} not {len(reviewed)}")
        else:
            ok(f"{g['total']} strings reviewed by the reviewer")
        # A source change makes the translation stale; a stale string isn't reviewed; translating it again clears it.
        lead.json("l10n_set", locale="en", entries={"hud.carve": "Carve stone"})
        s = lead.json("l10n_get", locale="de", status="stale")
        if [e["key"] for e in s["entries"]] != ["hud.carve"]:
            fail(f"stale after the source changed: {[e['key'] for e in s['entries']]}")
        else:
            ok("hud.carve is stale after the source changed")
        expect_error(rev, "l10n_set", {"locale": "de", "review": ["hud.carve"]}, "stale", "reviewing a stale string")
        c = lead.json("l10n_check", ui=False)
        if not any(f["rule"] == "stale" and f.get("key") == "hud.carve" for f in c["tables"]["findings"]):
            fail(f"l10n_check doesn't name the stale string: {c['tables']['by_rule']}")
        tr.json("l10n_set", locale="de", entries={"hud.carve": "Stein meißeln"})
        if lead.json("l10n_get", locale="de", status="stale")["total"]:
            fail("still stale after translating again")
        else:
            ok("translating again clears stale")
        # Undo takes a translation back (the journal holds the file's text).
        err, j = raw(tr, "world_undo", {"count": 1})
        back = tr.json("l10n_get", locale="de", keys=["hud.carve"])["entries"][0]["text"]
        want = de_entries["hud.carve"]
        want = want.get("text") if isinstance(want, dict) else want
        if err or back != want:
            fail(f"world_undo: {str(j)[:300]}; hud.carve is {back!a}, not {want!a}")
        else:
            ok(f"world_undo took the translation back to {back!a}")
        # Put the source and the German back as committed for the interface pass.
        lead.json("l10n_set", locale="en", entries={"hud.carve": "Carve"})
        tr.json("l10n_set", locale="de", entries={"hud.carve": de_entries["hud.carve"]})
        # Tables: no errors (missing keys, placeholders, undefined, unknown).
        c = lead.json("l10n_check", ui=False, max_findings=60)
        summary["tables"] = c["tables"]["by_rule"]
        summary["locales"] = c["locales"]
        for f in c["tables"]["findings"]:
            if f["severity"] == "error":
                fail(f"tables [{f['rule']}] {f.get('locale')} {f.get('key')} {f['message']}")
        print(f"tables: {c['tables']['errors']} errors, {c['tables']['warnings']} warnings {c['tables']['by_rule']}; game keys {c.get('game_keys')}")
        if c["tables"]["by_rule"].get("hardcoded"):
            fail(f"screens still write words themselves: {[f['message'] for f in c['tables']['findings'] if f['rule'] == 'hardcoded'][:5]}")
        # The interface in every language, the start state then a busy one.
        summary["ui_start"] = ui_pass(lead, "start", ["en", "de", "qps-ploc"])
        for tool, a in [("play_command", {"action": "place", "stone": "warding", "cell": "top:12,9"}), ("play_ui", {"press": "build_pillar"}),
                        ("play_ui", {"press": "pause_button"}), ("play_ui", {"select": "top:12,9"})]:
            err, j = raw(lead, tool, a)
            if err:
                fail(f"busy state: {tool} {a}: {str(j)[:300]}")
        summary["ui_busy"] = ui_pass(lead, "busy", ["en", "de", "qps-ploc"])
        for tool, a in [("play_ui", {"press": "deselect"}), ("play_ui", {"press": "resume_button"})]:
            raw(lead, tool, a)
        # The player's language from the settings: English -> German, captured, then back.
        caps = []
        # The same input device (the pad) and camera for every capture: only the language changes.
        act(lead, {"steps": [{"press": "pause"}, {"press": "back"}]})
        capture(lead, out, "en_hud", caps)
        act(lead, {"steps": [{"press": "pause"}]})
        capture(lead, out, "en_pause", caps)
        act(lead, {"steps": [{"activate": "settings"}]})
        capture(lead, out, "en_settings", caps)
        act(lead, {"steps": [{"activate": "language"}]})
        g = lead.json("l10n_get")
        if g.get("game_locale") != "de":
            fail(f"the Language button left the game in {g.get('game_locale')}")
        else:
            ok("the settings' Language button switched the game to de")
        tree = lead.json("ui_tree", screen="settings")
        lang = next((w for w in tree["widgets"] if w.get("text_key") == "settings.language"), None)
        if not lang or "Deutsch" not in lang.get("text", ""):
            fail(f"the settings don't say Deutsch: {lang}")
        # The same focus as the English captures: only the language differs.
        act(lead, {"steps": [{"focus": "ui_scale"}]})
        capture(lead, out, "de_settings", caps)
        act(lead, {"steps": [{"press": "back"}, {"focus": "resume"}]})
        capture(lead, out, "de_pause", caps)
        act(lead, {"steps": [{"press": "back"}]})
        capture(lead, out, "de_hud", caps)
        # The pseudo-locale for a capture: ui_check's, laid out by ui_tree; captures need the game's own
        # language, so the pseudo pass is measured (ui_pass) rather than captured.
        act(lead, {"steps": [{"press": "pause"}, {"activate": "settings"}, {"activate": "language"}, {"press": "back"}, {"press": "back"}]})
        summary["captures"] = caps
        g = lead.json("l10n_get")
        summary["game_locale_after"] = g.get("game_locale")
        for m in (lead, tr, rev, other):
            m.close()
    if eng.exit_code not in (0, None):
        fail(f"the engine exited with {eng.exit_code}")


def attacks(exe, port, adapter, out, summary):
    """The lead review R2's probes, kept (a second engine, with --strict-budget: an engine-thread
    stall fails the run):
    - the game's own nested messages (the HUD's hover) show words, never raw @keys;
    - a plain agent can't relabel a reviewed table as another language, nor save a table that
      doesn't parse (which hid every review from the gate);
    - ui_check {pseudo} and l10n_check with the pseudo-locale over a table at the format's size
      (about 4,000 keys) never hold the engine thread."""
    print("\nreview R2 probes (strict budget):")
    work = out / "work-attacks"
    if work.exists():
        shutil.rmtree(work)
    assets = work / "assets"
    shutil.copytree(ASSETS, assets)
    (assets / "strings" / "de.json").unlink(missing_ok=True)
    args = ["--headless", "--adapter", adapter, "--strict-budget", "--assets", str(assets), "--captures", str(work / "captures"), "--worlds", str(work / "worlds"),
            "--saves", str(work / "saves"), "--lead", "director", "--reviewer", "rev", "--", "--seed", "7", "--mute", "--intro", "off"]
    with Engine(exe, port, out / "engine-attacks.log", *args, cwd=str(work)) as eng:
        lead, rev, plain = eng.mcp("director"), eng.mcp("rev"), eng.mcp("plain")
        lead.json("agent_hello", name="director", role="lead")
        rev.json("agent_hello", name="rev", role="reviewer")
        plain.json("agent_hello", name="plain", role="translator")
        lead.call("time_control", action="pause")

        # Blocker 1: every text the HUD shows over the board is words.
        tree = lead.json("ui_tree", surface="game", screen="hud")
        texts = []

        def walk(n):
            if isinstance(n, dict):
                if isinstance(n.get("text"), str):
                    texts.append(n["text"])
                for v in n.values():
                    walk(v)
            elif isinstance(n, list):
                for v in n:
                    walk(v)

        walk(tree)
        raw_keys = [t for t in texts if "@" in t]
        if not texts or raw_keys:
            fail(f"the HUD shows raw keys: {raw_keys[:4]} ({len(texts)} texts)")
        else:
            ok(f"none of the HUD's {len(texts)} texts shows a raw @key")

        # Major 2: a reviewed table can't be relabelled as another language by a plain agent.
        err, j = raw(plain, "l10n_set", {"locale": "de", "create": {"name": "Deutsch"}, "entries": {"hud.menu": "Menue", "pause.resume": "Weiter"}})
        if err:
            fail(f"the translator starts a German table: {str(j)[:300]}")
        err, j = raw(rev, "l10n_set", {"locale": "de", "review": ["hud.menu", "pause.resume"]})
        if err:
            fail(f"the reviewer reviews two strings: {str(j)[:300]}")
        relabel = {"path": "assets/strings/de.json", "ops": [{"find": '"locale": "de"', "replace": '"locale": "fr"'}]}
        expect_error(plain, "project_patch", relabel, "not_reviewer", "a plain agent relabels the reviewed German table as French")
        o = lead.json("l10n_get")
        locs = sorted(t["locale"] for t in o["tables"])
        if locs != ["de", "en"]:
            fail(f"the tables after the refused relabel: {locs}")
        else:
            ok("the table is still German")
        # Minor 5: a table that doesn't parse is never saved, allow_errors or not.
        text = (assets / "strings" / "de.json").read_text(encoding="utf-8")
        broken = text.replace('"format"', '"bogus": 1,\n  "format"', 1)
        err, j = raw(plain, "project_write", {"path": "assets/strings/de.json", "text": broken, "allow_errors": True})
        if not err:
            fail(f"a string table that doesn't parse was saved with allow_errors: {str(j)[:300]}")
        else:
            ok(f"a string table that doesn't parse is refused even with allow_errors ({j.get('error') if isinstance(j, dict) else j})")
        err, j = raw(plain, "l10n_set", {"locale": "de", "entries": {"pause.restart_run": "Lauf neu starten"}})
        if err:
            fail(f"the translator goes on working after the refused draft: {str(j)[:300]}")
        else:
            ok("the translator goes on working (the reviews are intact)")

        # Major 3: the pseudo-locale over a big table never holds the engine thread.
        en = json.loads((assets / "strings" / "en.json").read_text(encoding="utf-8"))
        for i in range(3700):
            en["strings"][f"filler.k{i}"] = f"The quick brown fox {{a}} jumps over the lazy dog {{b:.1}} while the wraiths rise from the graveyard {{c:pct}} and dust gathers on the slab {{d}} n{i}"
        big = json.dumps(en, ensure_ascii=False, indent=1)
        err, j = raw(lead, "project_write", {"path": "assets/strings/en.json", "text": big})
        if err:
            fail(f"the lead writes a {len(en['strings'])}-key source table: {str(j)[:300]}")
        time.sleep(3.0)
        lead.call("time_control", action="step", ticks=1)
        time.sleep(1.0)
        base = {"surface": "game", "screens": ["hud"], "targets": ["deck", "1080p", "4k"], "rules": ["text_overflow"]}
        for label, a in (("ui_check", base), ("ui_check {pseudo}", dict(base, pseudo=True)), ("ui_check {locale: qps-ploc}", dict(base, locale="qps-ploc"))):
            err, j = raw(lead, "ui_check", a)
            if err:
                fail(f"{label} over the big table: {str(j)[:300]}")
        err, j = raw(lead, "l10n_check", {"locales": ["qps-ploc"], "targets": ["deck", "1080p", "4k"], "groups": [["hud"]], "rules": ["text_overflow"]})
        if err:
            fail(f"l10n_check qps-ploc over the big table: {str(j)[:300]}")
        # The Language button with the big table: a reference, nothing built on the engine thread.
        act(lead, {"steps": [{"press": "pause"}, {"activate": "settings"}, {"activate": "language"}, {"activate": "language"}, {"press": "back"}, {"press": "back"}]})
        time.sleep(0.5)
        err, st = raw(lead, "trace_incidents", {"code": "engine.stall"})
        stalls = (st.get("incidents") or st.get("items") or []) if isinstance(st, dict) else []
        summary["attacks"] = {"keys": len(en["strings"]), "stalls": len(stalls)}
        if err or stalls:
            fail(f"engine-thread stalls with a {len(en['strings'])}-key table: {str(st)[:400]}")
        else:
            ok(f"no engine-thread stall with a {len(en['strings'])}-key table (ui_check plain, pseudo and qps-ploc, l10n_check, the Language button)")
        for m in (lead, rev, plain):
            m.close()
    if eng.exit_code not in (0, None):
        fail(f"the strict-budget engine exited with {eng.exit_code} (a stall on the engine thread)")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7620)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe")
    ap.add_argument("--out", default=str(ROOT / "target" / "l10n-check"))
    ap.add_argument("--make-de", help="a JSON of German entries: write games/sepulcra/assets/strings/de.json through l10n_set")
    a = ap.parse_args()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target_dir / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p sepulcra-app")
    if not a.make_de and not DE.exists():
        sys.exit(f"{DE} is missing: run with --make-de first")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    summary = {}
    flow(exe, a.port, a.adapter, out, summary, a.make_de)
    if not a.make_de:
        attacks(exe, a.port + 1, a.adapter, out, summary)
    summary["failures"] = failures
    (out / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\n{'FAIL' if failures else 'PASS'}: {len(failures)} failures (summary: {out / 'summary.json'})")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
