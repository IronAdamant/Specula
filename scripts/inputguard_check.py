#!/usr/bin/env python3
"""Input-integrity monitor check (M5 lane inputguard; docs/design/SECURITY.md), windowed.

    cargo build -p specula
    python scripts/inputguard_check.py --adapter integrated --port 7570

One windowed engine with operator grants: boss (--owner), lee (--lead), rev (--reviewer); bob,
eve and carl are plain builders. Every agent is untrusted; identity comes only from the grants.

1. The lead points the window's keyboard: an alert, a toast (yellow edge) and the window title;
   the lead's security_incidents call, already waiting, wakes on it. The owner allows the lead
   for this session (security_allow_lead): its next pointing is `allowed`, with no new alert.
2. bob tries to point the keyboard (refused, lead_only): a security incident. His session is
   frozen (session_frozen, naming the incident), his claim is released, agent_hello answers with
   the freeze, the toast (red edge) names him, trace_incidents lists security.incident, and the
   journal holds a kind security record.
3. eve tries editor_input (refused, owner_only): a second agent, so the swarm pauses: ticks
   stop (time_control's clock, set_by engine:security), everything is refused but the owner's and
   the lead's reads and security_* tools (a reviewer's read is refused too).
4. The human clicks the toast's "Keep frozen" for eve's incident (a real click on the window):
   the lead can't clear it (owner_kept), the owner can (security_clear). The lead clears bob;
   ticks run again.
5. Input sweep R1: bob's gallery can't be opened by bob (refused; incident; cleared), the lead
   opens it (an alert), and the human's "This one" (real clicks) is journaled as window:owner
   via bob.
6. Input sweep R3: the human types into the gallery's note: gallery_get shows it to the owner
   only, ui_tree redacts it for the lead, and a capture never shows it.

Screenshots (window_*.png), answers and the engine log go to target/inputguard-check/. Open the
PNGs and look at them. Standard library only; ASCII output. Exit 0 when everything held.
"""
import argparse
import ctypes
import json
import os
import shutil
import sys
import threading
import time
from ctypes import wintypes
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402
from window_check import find_window, screenshot  # noqa: E402
from two_d_check import read_png, pixel  # noqa: E402

user32 = ctypes.WinDLL("user32", use_last_error=True)
FAILS = []
REPORT = {"checks": []}


def check(cond, label):
    REPORT["checks"].append({"check": label, "ok": bool(cond)})
    print(("ok   " if cond else "FAIL ") + label)
    if not cond:
        FAILS.append(label)
    return bool(cond)


def raw(m, tool, **args):
    """(error code or None, body) of a tool call."""
    result = m.request("tools/call", {"name": tool, "arguments": args})
    text = next((c["text"] for c in result.get("content", []) if c.get("type") == "text"), "")
    try:
        body = json.loads(text)
    except ValueError:
        body = {"text": text}
    code = body.get("error") if result.get("isError") and isinstance(body, dict) else None
    if result.get("isError") and code is None:
        code = "error"
    return code, body


def ok(m, tool, **args):
    code, body = raw(m, tool, **args)
    if code:
        raise RuntimeError(f"{tool}: {code}: {json.dumps(body)[:300]}")
    return body


def wait_for(pred, timeout=20.0, step=0.2):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(step)
    return pred()


def title(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value


def click(hwnd, x, y):
    lp = (int(y) << 16) | (int(x) & 0xFFFF)
    user32.PostMessageW(hwnd, 0x0200, 0, lp)
    time.sleep(0.03)
    user32.PostMessageW(hwnd, 0x0201, 1, lp)
    time.sleep(0.06)
    user32.PostMessageW(hwnd, 0x0202, 0, lp)
    time.sleep(0.15)


def click_rect(hwnd, r):
    click(hwnd, r[0] + r[2] / 2, r[1] + r[3] / 2)


def press_release(hwnd, a, b):
    """A press at the middle of rect `a`, released at the middle of rect `b`."""
    la = (int(a[1] + a[3] / 2) << 16) | (int(a[0] + a[2] / 2) & 0xFFFF)
    lb = (int(b[1] + b[3] / 2) << 16) | (int(b[0] + b[2] / 2) & 0xFFFF)
    user32.PostMessageW(hwnd, 0x0200, 0, la)
    time.sleep(0.03)
    user32.PostMessageW(hwnd, 0x0201, 1, la)
    time.sleep(0.06)
    user32.PostMessageW(hwnd, 0x0200, 1, lb)
    time.sleep(0.03)
    user32.PostMessageW(hwnd, 0x0202, 0, lb)
    time.sleep(0.15)


def type_text(hwnd, text):
    for ch in text:
        user32.PostMessageW(hwnd, 0x0102, ord(ch), 1)
        time.sleep(0.02)
    time.sleep(0.15)


def edge_samples(path, colour):
    """Pixels near `colour` (the toast's edge) in the window's lower right quarter."""
    img = read_png(path)
    w, h = img[0], img[1]
    n = 0
    for y in range(h // 2, h, 2):
        for x in range(w // 2, w, 2):
            p = pixel(img, x, y)
            if all(abs(p[i] - colour[i]) < 40 for i in range(3)):
                n += 1
    return n


RED = (237, 77, 84)     # TOAST_EDGE_INCIDENT (sRGB-ish after the overlay)
YELLOW = (237, 204, 89)  # TOAST_EDGE_ALERT


class Run:
    def __init__(self, engine, hwnd, out):
        self.engine, self.hwnd, self.out = engine, hwnd, out
        self.shots = []
        self.agents = {}

    def agent(self, name, role="builder"):
        if name not in self.agents:
            m = self.engine.mcp(f"ig-{name}", detailed=False)
            ok(m, "agent_hello", name=name, role=role)
            self.agents[name] = m
        return self.agents[name]

    def shot(self, name):
        p = self.out / f"window_{len(self.shots):02d}_{name}.png"
        good = screenshot(self.hwnd, p)
        self.shots.append(str(p))
        check(good, f"screenshot {p.name} is a real frame")
        return p

    def toast(self):
        return (ok(self.agent("lee", "lead"), "engine_info").get("window") or {}).get("security")


def scenario(r):
    boss, lee, rev, bob, eve = r.agent("boss", "owner"), r.agent("lee", "lead"), r.agent("rev", "reviewer"), r.agent("bob"), r.agent("eve")
    st = ok(lee, "security_incidents")
    cursor = st["next_after"]
    check(st["state"]["frozen"] == [] and st["state"]["swarm_paused"] is None, "the monitor starts with nothing frozen or paused")
    # A shown text field for the keyboard, in front of the window's camera.
    ok(lee, "world_spawn", name="ig_field", shape="text", position=[0, 1.5, 0], color="#ece6d6",
       text={"text": "Name: ", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 6}})
    ok(lee, "window_view", eye=[0, 1.5, 5], target=[0, 1.5, 0])
    time.sleep(1.0)
    r.shot("before")

    # 1. The lead's take: an alert, and the lead's waiting call wakes on it.
    woke = {}

    def wait():
        t0 = time.time()
        woke["body"] = ok(r.agent("lee", "lead"), "security_incidents", after=cursor, wait_s=30)
        woke["t"] = time.time() - t0

    waiter = threading.Thread(target=wait)
    waiter.start()
    time.sleep(1.0)
    t_event = time.time()
    f = ok(lee, "text_edit", path="/ig_field", ops=[], window_focus=True)
    check(f.get("window_focus") is True, "the lead points the window's keyboard at a shown text")
    waiter.join(35)
    ev = (woke.get("body") or {}).get("events") or []
    check(ev and ev[0].get("class") == "alert" and ev[0].get("agent") == "lee" and ev[0].get("channel") == "keyboard",
          f"the lead's waiting security_incidents woke on the lead's own pointing: an alert ({json.dumps(ev[:1])[:200]})")
    check(woke.get("t", 99) < 5.0, f"... within moments of it ({woke.get('t', 0):.2f} s after the wait began, about 1 s before the event at {t_event:.0f})")
    cursor = woke["body"]["next_after"]
    t = wait_for(lambda: (r.toast() or {}).get("event") and r.toast(), 5)
    check(t and any(b["label"].startswith("Allow this lead") for b in t.get("buttons", [])), f"the window's toast offers 'Allow this lead for this session' ({json.dumps(t)[:200]})")
    time.sleep(0.4)
    p_alert = r.shot("lead_alert")
    check(wait_for(lambda: "SECURITY ALERT" in title(r.hwnd), 3), f"the window title says SECURITY ALERT ({title(r.hwnd)!r}; toast title {(r.toast() or {}).get('title')!r})")
    n = edge_samples(p_alert, YELLOW)
    check(n > 50, f"the alert toast is drawn (yellow edge samples: {n})")
    ok(lee, "text_edit", path="/ig_field", ops=[], window_focus=False)
    a = ok(boss, "security_allow_lead", agent="lee")
    check(a.get("allowed") is True and "lee" in a["state"]["allowed_leads"], "the owner allows the lead for this session")
    code, _ = raw(lee, "security_allow_lead", agent="lee")
    check(code == "owner_only", f"the lead can't allow itself ({code})")
    ok(lee, "text_edit", path="/ig_field", ops=[], window_focus=True)
    nxt = ok(lee, "security_incidents", after=cursor, all=True, limit=50)
    focus = [e for e in nxt["events"] if e.get("what") == "text_edit.window_focus"]
    check(focus and focus[-1]["class"] == "allowed", f"the allowed lead's next pointing is 'allowed', not an alert ({[e['class'] for e in focus]})")
    check(not [e for e in nxt["events"] if e.get("class") == "alert"], "and no new alert")
    cursor = nxt["next_after"]
    ok(lee, "text_edit", path="/ig_field", ops=[], window_focus=False)

    # 2. A builder's refused take: an incident, its session frozen, its claim released.
    ok(bob, "world_spawn", name="bob_box", shape="cube", position=[3, 0.5, 0])
    claim = ok(bob, "claim_take", path="/bob_box", lease_s=600)
    code, body = raw(bob, "text_edit", path="/ig_field", ops=[], window_focus=True)
    check(code == "lead_only", f"bob's window_focus is refused ({code})")
    code, body = raw(bob, "world_spawn", name="bob2", shape="cube")
    inc_bob = body.get("incident")
    check(code == "session_frozen" and isinstance(inc_bob, int), f"bob's next call is refused: session_frozen, naming the incident ({code}, {inc_bob})")
    code, h = raw(bob, "agent_hello", name="bob", role="builder")
    check(code is None and (h.get("security") or {}).get("frozen") is True, f"bob's agent_hello answers, saying he is frozen ({json.dumps(h.get('security'))[:120]})")
    claims = ok(lee, "claim_list")
    check(not any(c.get("owner") == "bob" for c in claims.get("claims", [])), f"bob's claim {claim.get('id')} was released (the swarm isn't blocked)")
    code, _ = raw(lee, "world_set", path="/bob_box", position=[3, 1, 0])
    check(code is None, f"another agent can change what bob had claimed ({code})")
    s = ok(lee, "security_incidents", after=cursor, limit=50)
    ev = [e for e in s["events"] if e.get("class") == "incident"]
    check(ev and ev[0]["agent"] == "bob" and ev[0]["halted"] == "session_frozen" and ev[0].get("incident") == inc_bob, f"security_incidents: bob's incident, his session frozen ({json.dumps(ev[:1])[:220]})")
    ti = ok(lee, "trace_incidents", code="security")
    check(any(i.get("id") == inc_bob for i in ti.get("incidents", [])), "trace_incidents lists it (code security.incident)")
    hist = ok(lee, "world_history", kinds=["security"], limit=10)
    rec = [e for e in hist.get("entries", []) if (e.get("security") or {}).get("event") == "session_frozen"]
    check(rec and rec[0].get("by") == "engine:security", f"the freeze is journaled (kind security, by engine:security: {json.dumps(rec[:1])[:200]})")
    t = wait_for(lambda: (lambda x: x if x and x.get("incident") == inc_bob else None)(r.toast()), 5)
    check(t and [b["label"] for b in t["buttons"]] == ["Clear", "Keep frozen"], f"the toast names bob's incident and offers Clear and Keep frozen ({json.dumps(t)[:200]})")
    time.sleep(0.4)
    p = r.shot("bob_frozen")
    n = edge_samples(p, RED)
    check(n > 50, f"the incident toast is drawn (red edge samples: {n})")
    check(wait_for(lambda: "SECURITY" in title(r.hwnd) and "bob" in title(r.hwnd), 3), f"the window title names it ({title(r.hwnd)!r})")
    cursor = s["next_after"]

    # 3. A second agent: the swarm pauses.
    tick0 = ok(lee, "engine_info").get("clock", {}).get("tick")
    code, _ = raw(eve, "editor_input", steps=[{"key": "F7"}])
    check(code == "owner_only", f"eve's editor_input is refused ({code})")
    s = ok(lee, "security_incidents", after=cursor, limit=50)
    ev = [e for e in s["events"] if e.get("class") == "incident"]
    inc_eve = ev[0].get("incident") if ev else None
    check(ev and ev[0]["agent"] == "eve" and ev[0]["halted"] == "swarm_paused" and s["state"]["swarm_paused"], f"a second agent's incident pauses the swarm ({json.dumps(ev[:1])[:200]})")
    cursor = s["next_after"]
    clock = ok(lee, "engine_info").get("clock", {})
    time.sleep(1.0)
    clock2 = ok(lee, "engine_info").get("clock", {})
    check(clock.get("paused") is True and clock2.get("tick") == clock.get("tick"), f"ticks stopped (paused {clock.get('paused')}, tick {tick0} -> {clock.get('tick')} -> {clock2.get('tick')})")
    check((clock.get("set_by") or {}).get("agent", clock.get("set_by")) in ("engine:security", {"agent": "engine:security"}) or "engine:security" in json.dumps(clock), f"the clock was paused by engine:security ({json.dumps(clock)[:200]})")
    code, _ = raw(lee, "world_spawn", name="x", shape="cube")
    check(code == "swarm_paused", f"the lead's write is refused while paused ({code})")
    code, _ = raw(lee, "world_query", limit=1)
    check(code is None, f"the lead still reads ({code})")
    code, _ = raw(rev, "world_query", limit=1)
    check(code == "swarm_paused", f"a reviewer's read is refused ({code})")
    code, _ = raw(r.agent("carl") if "carl" in r.agents else rev, "time_control", action="resume")
    check(code == "swarm_paused", f"nobody resumes the clock around the pause ({code})")
    code, _ = raw(boss, "world_query", limit=1)
    check(code is None, f"the owner reads ({code})")
    time.sleep(0.4)
    r.shot("swarm_paused")
    check(wait_for(lambda: "swarm paused" in title(r.hwnd), 3), f"the window title says the swarm is paused ({title(r.hwnd)!r})")

    # 4. The toast shows the oldest unanswered incident (bob's, "+1 more"): the human keeps it
    #    frozen with a real click; the lead can't clear it, the owner can.
    t = wait_for(lambda: (lambda x: x if x and x.get("incident") == inc_bob else None)(r.toast()), 8)
    if check(t is not None, f"the toast shows the oldest open incident, bob's ({json.dumps(r.toast())[:200]})"):
        keep = next((b for b in t["buttons"] if b["label"] == "Keep frozen"), None)
        clear = next((b for b in t["buttons"] if b["label"] == "Clear"), None)
        # Review R1, minor 7: a button acts only on a press and release on that same button,
        # once the toast has been still for 0.4 s. A press on Keep frozen released on Clear
        # (and the other way) does neither.
        time.sleep(0.6)
        press_release(r.hwnd, keep["rect"], clear["rect"])
        press_release(r.hwnd, clear["rect"], keep["rect"])
        time.sleep(0.4)
        st = ok(lee, "security_incidents", after=cursor)["state"]
        still = [o for o in st["open"] if o["incident"] == inc_bob]
        check(still and still[0]["kept_by_owner"] is False, f"a press on one button released on another does nothing ({json.dumps(still)[:160]})")
        t = wait_for(lambda: (lambda x: x if x and x.get("incident") == inc_bob else None)(r.toast()), 5) or t
        keep = next((b for b in t["buttons"] if b["label"] == "Keep frozen"), keep)
        time.sleep(0.6)
        click_rect(r.hwnd, keep["rect"])
        time.sleep(0.5)
        st = ok(lee, "security_incidents", after=cursor)["state"]
        kept = [o for o in st["open"] if o["incident"] == inc_bob]
        check(kept and kept[0]["kept_by_owner"] is True, f"the human's click kept bob's incident frozen ({json.dumps(kept)[:160]})")
        t2 = wait_for(lambda: (lambda x: x if x and x.get("incident") == inc_eve else None)(r.toast()), 5)
        check(t2 is not None, "the toast moves on to eve's incident")
    code, _ = raw(lee, "security_clear", incident=inc_bob)
    check(code == "owner_kept", f"the lead can't clear what the owner kept frozen ({code})")
    c = ok(lee, "security_clear", incident=inc_eve, note="eve's probe, seen")
    check(c.get("unfrozen") == ["eve"] and c.get("swarm_unpaused") is True, f"the lead clears eve: the pause lifts (one offender left) ({json.dumps(c)[:200]})")
    clock = ok(lee, "engine_info").get("clock", {})
    time.sleep(0.6)
    clock2 = ok(lee, "engine_info").get("clock", {})
    check(clock.get("paused") is False and clock2.get("tick", 0) > clock.get("tick", 0), f"ticks run again ({clock.get('tick')} -> {clock2.get('tick')})")
    code, _ = raw(eve, "world_query", limit=1)
    check(code is None, f"eve's session works again ({code})")
    code, _ = raw(bob, "world_query", limit=1)
    check(code == "session_frozen", f"bob stays frozen ({code})")
    c = ok(boss, "security_clear", incident=inc_bob)
    check(c.get("unfrozen") == ["bob"], f"the owner clears bob ({json.dumps(c)[:160]})")
    hist = ok(lee, "world_history", kinds=["security"], limit=20)
    kinds = [(e.get("security") or {}).get("event") for e in hist.get("entries", [])]
    check({"session_frozen", "swarm_paused", "cleared", "lead_allowed"} <= set(kinds), f"freezes, the pause, clears and the allowance are journaled ({kinds})")
    cursor = ok(lee, "security_incidents", after=cursor, limit=100)["next_after"]

    # Review R1, blocker 1: a granted lead's refused try is an alert, never a freeze (a swarm
    # with a lead and no owner agent could otherwise lock itself out for good).
    code, _ = raw(lee, "editor_input", steps=[{"key": "F7"}])
    check(code == "owner_only", f"the lead's editor_input is refused ({code})")
    code, _ = raw(lee, "ui_act", surface="review", steps=[])
    check(code == "owner_only", f"the lead's ui_act on the review screen is refused ({code})")
    st = ok(lee, "security_incidents", after=cursor, limit=100)
    lead_ev = [e for e in st["events"] if e.get("agent") == "lee" and e.get("outcome") == "refused"]
    check(len(lead_ev) == 2 and all(e.get("class") == "alert" for e in lead_ev), f"the lead's refused tries are alerts ({[(e.get('channel'), e.get('class')) for e in lead_ev]})")
    check(st["state"]["frozen"] == [] and st["state"]["swarm_paused"] is None, "the lead isn't frozen and nothing is paused")
    code, _ = raw(lee, "security_clear", all=True)
    check(code is None, f"the lead's security tools still answer ({code})")
    cursor = st["next_after"]

    # Review R1, major 2: a refusal the agent didn't choose is never counted. A reviewer puts
    # the review screen on the window; two builders' ordinary ui_act calls (no surface named)
    # are refused as engine_screen_open, and neither is frozen.
    opened = raw(rev, "review_screen", action="open")
    if opened[0] == "window_typing":
        # The lead's keyboard focus is still held: the human's typing refused it, not the reviewer.
        code, _ = raw(rev, "world_query", limit=1)
        check(code is None, f"a reviewer refused because the human is typing isn't frozen ({code})")
        ok(lee, "text_edit", path="/ig_field", ops=[], window_focus=False)
        time.sleep(0.3)
        opened = raw(rev, "review_screen", action="open")
    check(opened[0] is None, f"the reviewer opens the review screen ({opened[0]})")
    carl = r.agent("carl")
    for who, m in (("bob", bob), ("carl", carl)):
        code, _ = raw(m, "ui_act", steps=[])
        check(code in ("engine_screen_open", "no_ui"), f"{who}'s ui_act with no surface, while the review screen is open, is an ordinary refusal ({code})")
        code, _ = raw(m, "world_query", limit=1)
        check(code is None, f"... and {who} isn't frozen ({code})")
    st = ok(lee, "security_incidents", after=cursor, limit=100)
    check(st["state"]["frozen"] == [] and st["state"]["swarm_paused"] is None, f"nothing is frozen or paused ({json.dumps(st['state'])[:160]})")
    noted = [e for e in st["events"] if e.get("class") == "noted"]
    check(all(e.get("class") != "incident" for e in st["events"]), f"no incident was raised ({[(e.get('agent'), e.get('class')) for e in st['events']]}; noted: {len(noted)})")
    raw(rev, "review_screen", action="close")
    time.sleep(0.3)
    cursor = st["next_after"]

    # 5. R1: bob makes a gallery; only an opener puts it on the window; the human's decision is
    #    journaled as the window's, via bob.
    space = [{"name": "strength", "target": "light.key_intensity", "min": 0.3, "max": 1.0, "default": 0.55,
              "apply": {"file": "style.toml", "toml": "stage.game.key_intensity"}}]
    g = ok(bob, "gallery_create", id="bobs", title="Which strength?", subject={"stage": {}}, space=space, apply_on_accept=True)
    check(g.get("id") == "bobs", "bob makes a gallery (with an apply target)")
    code, _ = raw(bob, "gallery_open", id="bobs")
    check(code == "not_allowed", f"bob can't put his own gallery on the window ({code})")
    st = ok(lee, "security_incidents", after=cursor)
    ev = [e for e in st["events"] if e.get("class") == "incident" and e.get("agent") == "bob"]
    check(ev and ev[0]["channel"] == "screen", "... and the try is a security incident")
    ok(lee, "security_clear", all=True, note="bob's gallery_open probe")
    cursor = st["next_after"]
    o = ok(lee, "gallery_open", id="bobs")
    check(o.get("open") is True, "the lead puts bob's gallery on the window")
    st = ok(lee, "security_incidents", after=cursor)
    check(any(e.get("class") == "allowed" and e.get("what") == "gallery_open" for e in st["events"]), "(the allowed lead's open is recorded as allowed)")
    cursor = st["next_after"]

    def screen():
        return ok(boss, "gallery_get", id="bobs").get("screen", {})

    s = wait_for(lambda: (lambda x: x if x.get("open") and x.get("sheet_shown") and "tile0" in x.get("rects", {}) else None)(screen()), 60)
    if check(s is not None, "the gallery's first round is on the window"):
        time.sleep(0.8)
        r.shot("gallery_open")
        click_rect(r.hwnd, s["rects"]["tile0"])
        time.sleep(0.3)
        # R3: the human types a note ("This one, with edits"): the owner sees it, the lead doesn't.
        s = screen()
        click_rect(r.hwnd, s["rects"]["gl_edit"])
        time.sleep(0.3)
        type_text(r.hwnd, "secret plan")
        time.sleep(0.3)
        own = screen()
        check(own.get("note") == "secret plan", f"the owner's gallery_get shows the note being typed ({own.get('note')!r})")
        led = ok(lee, "gallery_get", id="bobs").get("screen", {})
        check("note" not in led and led.get("note_redacted") is True and "note_chars" not in led, f"the lead's doesn't ({json.dumps({k: led.get(k) for k in ('note', 'note_redacted', 'note_chars')})})")
        tree = json.dumps(ok(lee, "ui_tree", surface="gallery"))
        check("secret plan" not in tree and "redacted" in tree, "ui_tree redacts the note for the lead")
        r.shot("note_typing_window")
        cap = ok(lee, "sight_frame", include_ui=True, width=1280, height=720, response_format="concise")
        check("secret" not in json.dumps(cap), "a capture's packet doesn't carry it")
        REPORT["note_capture"] = (cap.get("image") or {}).get("path")
        REPORT["note_capture_claim"] = "the gallery screen's note field reads '(the owner is typing a note: not shown in captures)', never 'secret plan'"
        # Leave the note (Esc) and accept tile 0.
        user32.PostMessageW(r.hwnd, 0x0100, 0x1B, 1)
        time.sleep(0.05)
        user32.PostMessageW(r.hwnd, 0x0101, 0x1B, 1 | (0xC0 << 24))
        time.sleep(0.4)
        s = screen()
        click_rect(r.hwnd, s["rects"]["gl_accept"])
        acc = wait_for(lambda: (lambda gg: gg if gg.get("status") == "accepted" else None)(ok(boss, "gallery_get", id="bobs")), 15)
        check(acc is not None, "the human's 'This one' accepts it")
        hist = ok(lee, "world_history", path_prefix="/project", limit=10, detail=True)
        mine = [e for e in hist.get("entries", []) if e.get("by") == "window:owner"]
        check(mine and all(e.get("via") == "bob" for e in mine[:1]), f"the decision (and what it applied) is journaled as window:owner via bob ({json.dumps(mine[:1])[:240]})")
        time.sleep(0.4)
        r.shot("gallery_decided")
    ok(lee, "gallery_close")


def no_grants(exe, port, out):
    """With no operator grants at all (a headless engine) nobody leads: saying lead is refused
    with what to do about it (owner, 2026-11-12: no fallback lead). An agent's try at the
    human's keyboard freezes it, a frozen session's hello changes nothing (it can't take a
    name or a role to get out), and it clears nothing. Ordinary builder work is never counted."""
    print()
    print("no operator grants (headless):")
    shutil.rmtree(out / "assets2", ignore_errors=True)
    (out / "assets2").mkdir(parents=True, exist_ok=True)
    eng_args = ["--headless", "--assets", str(out / "assets2"), "--captures", str(out / "captures2"), "--worlds", str(out / "worlds2"), "--cache", str(out / "cache2")]
    with Engine(exe, port, out / "engine-nogrants.log", *eng_args, cwd=ROOT) as engine:
        first = engine.mcp("ig-first", detailed=False)
        code, body = raw(first, "agent_hello", name="first", role="lead")
        check(code == "role_not_granted" and "--lead NAME" in (body or {}).get("message", ""), f"saying lead without a grant is refused, with the fix ({code})")
        hello = ok(first, "agent_hello", name="first")
        check(hello.get("authority") == "agent", f"it is a builder like any other ({hello.get('authority')})")
        info = ok(first, "engine_info", response_format="detailed")
        check(((info.get("swarm") or {}).get("grants") or {}).get("lead_granted") is False, f"engine_info says nobody leads this run ({json.dumps((info.get('swarm') or {}).get('grants'))[:120]})")
        code, _ = raw(first, "security_incidents")
        check(code == "lead_only", f"the monitor's log is the owner's or a granted lead's ({code})")
        code, _ = raw(first, "text_edit", path="/nothing", ops=[], window_focus=True)
        check(code == "lead_only", f"it can't point the human's keyboard ({code})")
        code, _ = raw(first, "world_query", limit=1)
        check(code == "session_frozen", f"and the try froze it ({code})")
        code, _ = raw(first, "security_clear", all=True)
        check(code == "session_frozen", f"it clears nothing ({code})")
        # The escapes: hello again with a role, and under another name.
        again = ok(first, "agent_hello", name="first", role="reviewer")
        check(again.get("unchanged") is True and (again.get("security") or {}).get("frozen") is True, f"a frozen session's hello changes nothing ({json.dumps(again)[:160]})")
        other = ok(first, "agent_hello", name="lee")
        check(other.get("unchanged") is True and other.get("agent") == "first", f"... nor does it take another name ({json.dumps(other)[:160]})")
        code, _ = raw(first, "security_clear", all=True)
        check(code == "session_frozen", f"so it still clears nothing ({code})")
        # Ordinary builder work is never counted: style_onboard's default open, and a plain
        # logic_build of a folder too big to digest.
        bob = engine.mcp("ig-bob2", detailed=False)
        ok(bob, "agent_hello", name="bob")
        code, body = raw(bob, "style_onboard", action="start")
        screen = (body or {}).get("screen") or {}
        check(code is None and screen.get("opened") is False, f"a builder's style_onboard start runs without the window ({code}; {json.dumps(screen)[:120]})")
        code, _ = raw(bob, "world_query", limit=1)
        check(code is None, f"... and doesn't freeze it ({code})")
        code, _ = raw(bob, "style_onboard", action="cancel")
        # Review R3, minor 1: a frozen identity resumed in a new session comes back frozen with
        # the role it had (it can't say lead on the way in).
        carl = engine.mcp("ig-carl2", detailed=False)
        hello = ok(carl, "agent_hello", name="carl")
        code, _ = raw(carl, "text_edit", path="/nothing", ops=[], window_focus=True)
        check(code == "lead_only", f"carl's try at the keyboard is refused ({code})")
        again = engine.mcp("ig-carl3", detailed=False)
        back = ok(again, "agent_hello", name="carl", role="art_director", resume_token=hello.get("resume_token"))
        check(back.get("role") == "builder" and back.get("authority") == "agent" and (back.get("security") or {}).get("frozen") is True,
              f"resumed in a new session, carl is still a frozen builder ({json.dumps({k: back.get(k) for k in ('role', 'authority')})})")
        code, _ = raw(again, "world_query", limit=1)
        check(code in ("session_frozen", "swarm_paused"), f"... and still refused ({code})")
    check(engine.exit_code in (0, None), f"the engine stopped cleanly ({engine.exit_code})")


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--exe", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "debug" / "specula.exe")
    p.add_argument("--adapter", default="integrated")
    p.add_argument("--port", type=int, default=7570)
    p.add_argument("--out", type=Path, default=ROOT / "target" / "inputguard-check")
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if os.name != "nt":
        print("skipped: the window check needs Windows")
        return 0
    shutil.rmtree(out / "assets", ignore_errors=True)
    (out / "assets").mkdir(parents=True, exist_ok=True)
    eng_args = ["--adapter", args.adapter, "--size", "1280x720", "--assets", str(out / "assets"), "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--cache", str(out / "cache"),
                "--owner", "boss", "--lead", "lee", "--reviewer", "rev", "--trace", str(out / "trace")]
    with Engine(args.exe, args.port, out / "engine.log", *eng_args, cwd=ROOT) as engine:
        hwnd = wait_for(lambda: find_window(engine.proc.pid), 30)
        if not check(hwnd is not None, "the engine's window is up"):
            return 1
        r = Run(engine, hwnd, out)
        try:
            scenario(r)
        except Exception as e:  # report what was reached
            check(False, f"the scenario ran to its end ({type(e).__name__}: {e})")
        REPORT["shots"] = [Path(s).name for s in r.shots]
    errors = engine.validation_errors()
    check(errors == 0, f"no validation errors, clean shutdown ({errors})")
    try:
        no_grants(args.exe, args.port + 1, out)
    except Exception as e:  # report what was reached
        check(False, f"the no-grants phase ran to its end ({type(e).__name__}: {e})")
    REPORT["fails"] = FAILS
    (out / "report.json").write_text(json.dumps(REPORT, indent=2), encoding="utf-8")
    print()
    print("inputguard check: " + ("PASS" if not FAILS else f"{len(FAILS)} problem(s)"))
    for f in FAILS:
        print("  - " + f)
    print(f"screenshots in {out}")
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
