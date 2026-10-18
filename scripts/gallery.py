#!/usr/bin/env python3
"""The visual history gallery: dated thumbnails of tests, assets and gameplay, kept in the repo.

Each entry is a folder `docs/gallery/<YYYY-MM-DD>-<slug>/` holding small JPEG thumbnails (about
640 px wide) and an `entry.json` (date, milestone, commit, topic, subject, title, a note on what is
shown and what changed, the source script or tool, the adapter, and the original file names). The
full-size originals are copied to `gallery-full/<entry>/` in the MAIN working tree (shared by every
git worktree; override with SPECULA_GALLERY_FULL), which is gitignored, so nothing is lost locally
while the public repo stays small. `docs/gallery/README.md` is generated from the entries: each
subject's progression over time, then newest first.

    python scripts/gallery.py add --date 2026-10-18 --time 10:19 --date-source "run id" \\
        --subject sepulcra --topic game --title "Sepulcra on the iGPU" \\
        --note "What is shown, and what changed since last time." \\
        --milestone M3 --commit d35288e --adapter integrated --source scripts/sepulcra_play.py \\
        target/sepulcra-play/integrated/captures/run-*/frame_00001.png::"Build phase" more.png ...
    python scripts/gallery.py index        # regenerate README.md from the entries
    python scripts/gallery.py check        # budgets, files, orphans, README up to date (exit 1 on a problem)
    python scripts/gallery.py dupes        # thumbnails that repeat an earlier entry of the same subject
    python scripts/gallery.py drop ENTRY FILE... [--note TEXT]   # remove thumbnails from an entry
    python scripts/gallery.py subjects     # the subject and topic names

A FILE may carry a caption after `::`. Thumbnails are made by specula-core's `thumb` example
(box filter in linear light, baseline JPEG; zero dependencies): it is built on first use into
CARGO_TARGET_DIR (or target/), or pass `--thumb PATH`. Use `--png` for pixel art or text that JPEG
would smear. Look at every image before adding it: drop black or broken frames, or say in the note
that they show a failure. `add` warns when a thumbnail repeats one of an earlier entry of the same
subject: leave it out and say "unchanged since <entry>" in the note instead.

An entry is built in a temporary folder and swapped in only when everything succeeded. Rebuilding
an existing entry needs `--force-replace`: its old thumbnails stay in git history (the repo grows),
and its old originals are kept beside the new ones as `<entry>.replaced-<time>`, never deleted.

Standard library only; output is ASCII.
"""
import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GALLERY = ROOT / "docs" / "gallery"
TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")

# Budgets for what the public repo carries: every file under docs/gallery counts.
IMAGE_BUDGET = 200 * 1024
ENTRY_BUDGET = 900 * 1024
TOTAL_BUDGET = 24 * 1024 * 1024
WIDTH = 640
MAX_THUMB_HEIGHT = 1600
SLUG = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
ENTRY_NAME = re.compile(r"^\d{4}-\d\d-\d\d-[a-z0-9]+(?:-[a-z0-9]+)*$")
MAX_SLUG = 60
# Two thumbnails whose 8x8 luma signatures differ by at most this much in every cell repeat each other.
DUPLICATE_CELL_DIFF = 1

# Subjects, in the order the README shows them: key -> (heading, one-line description).
SUBJECTS = {
    "sepulcra": ("Sepulcra over time", "The first game, a tower defence, from its first slice to now."),
    "ui": ("Interface and text over time", "Text, specula-ui and Sepulcra's HUD, menus and screens."),
    "creatures": ("Creatures and animation over time", "Sepulcra's creatures: clips, graphs, contact sheets."),
    "assets": ("Procedural 3D assets over time", "Recipes built by specula-model, from first shapes to tiers."),
    "lava-lamps": ("Lava lamps over time", "The fidelity benchmark: three lamps from basic to as real as possible."),
    "engine-tests": ("Engine test scenes over time", "Window checks, GPU test renders, sight checks and exit proofs."),
    "two-d": ("2D over time", "Sprites, tilemaps and vector shapes."),
    "physics": ("Physics over time", "specula-physics scenes with their collider and contact overlays."),
    "navigation": ("Navigation over time", "Navmeshes, paths and agents."),
    "bevy-era": ("Before Specula: the Bevy era", "Grok-Bevy, the tooling Specula grew out of."),
}
TOPICS = ["game", "engine test", "3D asset", "animation", "physics", "navigation", "UI", "effects", "2D", "sight",
          "performance", "audio", "tooling"]
ADAPTERS = ["integrated", "discrete", "both", "software", "n/a"]
ENTRY_KEYS = ["date", "time", "date_source", "approximate", "milestone", "commit", "subject", "topic", "title",
              "note", "source", "adapter", "images"]


def fail(msg):
    print(f"gallery: {msg}")
    sys.exit(1)


def full_root():
    """Where originals live: SPECULA_GALLERY_FULL, else `gallery-full/` in the main working tree (the
    parent of git's common directory), so lanes in their own worktrees all add to one place."""
    env = os.environ.get("SPECULA_GALLERY_FULL")
    if env:
        return Path(env)
    try:
        r = subprocess.run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"], cwd=ROOT,
                           capture_output=True, text=True, encoding="utf-8", errors="replace")
        common = Path(r.stdout.strip())
        if r.returncode == 0 and common.name == ".git" and common.is_dir():
            return common.parent / "gallery-full"
    except OSError:
        pass
    if (ROOT / ".git").is_file():
        # A linked worktree whose main tree can't be found (moved, git older than 2.31, a separate git
        # dir): falling back to this worktree would strand the originals when it is removed.
        fail("can't find the main working tree for gallery-full/ from this linked worktree: set "
             "SPECULA_GALLERY_FULL to the gallery-full folder of the main checkout")
    return ROOT / "gallery-full"


def inside(path, base):
    """Whether `path` resolves to a direct child of `base`."""
    return path.resolve().parent == base.resolve()


def slugify(text):
    s = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return s[:48].rstrip("-") or "entry"


def thumb_exe(given):
    if given:
        return Path(given)
    exe = TARGET / "release" / "examples" / ("thumb.exe" if sys.platform == "win32" else "thumb")
    if not exe.exists():
        print("building the thumb example (specula-core, release)...")
        subprocess.run(["cargo", "build", "--release", "-p", "specula-core", "--example", "thumb"], cwd=ROOT,
                       check=True)
    return exe


class ThumbError(Exception):
    pass


def run_thumb(exe, args, allow_errors=False):
    r = subprocess.run([str(exe)] + [str(a) for a in args], capture_output=True, text=True, encoding="utf-8",
                       errors="replace")
    lines = []
    for line in r.stdout.splitlines():
        if line.strip():
            try:
                lines.append(json.loads(line))
            except ValueError:
                raise ThumbError(f"thumb printed something that isn't JSON: {line[:200]!r}")
    if not allow_errors and (r.returncode != 0 or not lines or "error" in lines[-1]):
        raise ThumbError(f"thumb failed: {(r.stdout.strip() or r.stderr.strip())[:500]}")
    return lines


def load_entries():
    entries = []
    if not GALLERY.is_dir():
        return entries
    for d in sorted(GALLERY.iterdir()):
        f = d / "entry.json"
        if d.is_dir() and not d.name.startswith(".") and f.exists():
            e = json.loads(f.read_text(encoding="utf-8"))
            e["_dir"] = d.name
            entries.append(e)
    return entries


def sort_key(e):
    return (e["date"], e.get("time") or "", e["_dir"])


def tree_bytes(path):
    return sum(p.stat().st_size for p in path.rglob("*") if p.is_file()) if path.exists() else 0


def signatures(exe, paths):
    """8x8 luma signatures (64 ints) of the given thumbnails, by path string."""
    out = {}
    for i in range(0, len(paths), 64):
        for row in run_thumb(exe, ["stats"] + [str(p) for p in paths[i:i + 64]], allow_errors=True):
            if "sig" in row:
                out[row["path"]] = [int(row["sig"][k:k + 2], 16) for k in range(0, 128, 2)]
    return out


def repeats(a, b):
    return max(abs(x - y) for x, y in zip(a, b)) <= DUPLICATE_CELL_DIFF


def earlier_duplicates(exe, entry, folder, entries):
    """[(image file, earlier entry dir, earlier file)] for this entry's thumbnails that repeat a thumbnail of
    an earlier entry (by date and time) of the same subject."""
    key = (entry["date"], entry.get("time") or "")
    earlier = [e for e in entries if e["subject"] == entry["subject"] and (e["date"], e.get("time") or "") < key]
    theirs = [(e["_dir"], im["file"], GALLERY / e["_dir"] / im["file"]) for e in earlier for im in e["images"]]
    ours = [(im["file"], folder / im["file"]) for im in entry["images"]]
    sigs = signatures(exe, [p for _, _, p in theirs] + [p for _, p in ours])
    found = []
    for f, p in ours:
        mine = sigs.get(str(p))
        for d, g, q in theirs:
            other = sigs.get(str(q))
            if mine and other and repeats(mine, other):
                found.append((f, d, g))
                break
    return found


# ------------------------------------------------------------------------------------------------
# add

def valid_date(text):
    """Exactly YYYY-MM-DD and a real date (fromisoformat alone also takes 20261018)."""
    if not isinstance(text, str) or not re.fullmatch(r"\d{4}-\d\d-\d\d", text):
        return False
    try:
        datetime.date.fromisoformat(text)
    except ValueError:
        return False
    return True


def cmd_add(a):
    if not valid_date(a.date):
        fail(f"--date {a.date!r} is not YYYY-MM-DD")
    if a.time and not re.fullmatch(r"([01]\d|2[0-3]):[0-5]\d", a.time):
        fail(f"--time {a.time!r} is not HH:MM")
    if a.subject not in SUBJECTS:
        fail(f"--subject {a.subject!r} is not one of {', '.join(SUBJECTS)} (add new ones to SUBJECTS in this script)")
    if a.topic not in TOPICS:
        fail(f"--topic {a.topic!r} is not one of {', '.join(TOPICS)}")
    if a.adapter and a.adapter not in ADAPTERS:
        fail(f"--adapter {a.adapter!r} is not one of {', '.join(ADAPTERS)}")
    if not 16 <= a.width <= 1024:
        fail("--width must be 16..1024")
    slug = a.slug or slugify(a.title)
    if not SLUG.fullmatch(slug) or len(slug) > MAX_SLUG:
        fail(f"--slug {slug!r} must be lowercase letters, digits and single dashes, at most {MAX_SLUG} characters")
    name = f"{a.date}-{slug}"
    full_base = full_root()
    folder, full = GALLERY / name, full_base / name
    if not inside(folder, GALLERY) or not inside(full, full_base):
        fail(f"entry name {name!r} escapes the gallery")
    if folder.exists() and not a.force_replace:
        fail(f"{folder.relative_to(ROOT)} exists. Pick another --slug, or pass --force-replace to rebuild it "
             f"(its old thumbnails stay in git history, so the repo grows; prefer `drop` to remove images)")
    files = []
    for spec in a.files:
        path, _, caption = spec.partition("::")
        p = Path(path)
        if not p.is_file():
            fail(f"no such file: {path}")
        files.append((p, caption.strip()))
    if not files:
        fail("no files to add")
    exe = thumb_exe(a.thumb)
    old_bytes = tree_bytes(folder)
    tmp = Path(tempfile.mkdtemp(prefix="gallery-"))
    staged_full = full_base / f".{name}.new"
    staged_thumbs = GALLERY / f".{name}.new"
    try:
        # 1. Copy the sources first, so rebuilding an entry from its own originals is safe.
        src = tmp / "src"
        src.mkdir()
        copies = []
        for i, (p, caption) in enumerate(files, 1):
            original = re.sub(r"^\d\d-", "", p.name)  # rebuilding from gallery-full keeps its names
            c = src / f"{i:02d}-{original}"
            shutil.copy2(p, c)
            copies.append((c, original, caption))
        # 2. Thumbnails and entry.json, in the temporary folder.
        thumbs = tmp / "thumbs"
        thumbs.mkdir()
        images = []
        for i, (c, original, caption) in enumerate(copies, 1):
            ext = ".png" if a.png else ".jpg"
            out = thumbs / f"{i:02d}-{slugify(Path(original).stem)}{ext}"
            info = run_thumb(exe, [c, out, a.width, "--quality", a.quality, "--max-bytes", IMAGE_BUDGET])[-1]
            written = Path(info["out"])
            if info["bytes"] > IMAGE_BUDGET:
                fail(f"{written.name} is {info['bytes']} bytes, over the {IMAGE_BUDGET}-byte image budget (smaller --width)")
            if info["height"] > MAX_THUMB_HEIGHT:
                fail(f"{original} makes a {info['height']} px tall thumbnail (limit {MAX_THUMB_HEIGHT}): crop it or use a smaller --width")
            if info.get("format") == "png" and not a.png:
                print(f"note: {original} kept as PNG (JPEG couldn't keep its detail)")
            images.append({"file": written.name, "caption": caption, "original": original, "width": info["width"],
                           "height": info["height"], "bytes": info["bytes"]})
        entry = {
            "date": a.date, "time": a.time or "", "date_source": a.date_source, "approximate": bool(a.approximate),
            "milestone": a.milestone or "", "commit": a.commit or "", "subject": a.subject, "topic": a.topic,
            "title": a.title, "note": a.note, "source": a.source or "", "adapter": a.adapter or "", "images": images,
        }
        text = json.dumps(entry, indent=2, ensure_ascii=True) + "\n"
        (thumbs / "entry.json").write_text(text, encoding="utf-8", newline="\n")
        new_bytes = tree_bytes(thumbs)
        if new_bytes > ENTRY_BUDGET:
            fail(f"{name} is {new_bytes} bytes, over the {ENTRY_BUDGET}-byte entry budget: pick fewer images")
        others = [e for e in load_entries() if e["_dir"] != name]
        total = sum(tree_bytes(GALLERY / e["_dir"]) for e in others) + new_bytes
        if total > TOTAL_BUDGET:
            fail(f"the gallery would be {total} bytes, over its {TOTAL_BUDGET}-byte budget")
        for f, d, g in earlier_duplicates(exe, entry, thumbs, others):
            print(f"warning: {f} repeats {d}/{g}: leave it out and say 'unchanged since {d}' in the note")
        # 3. Swap in: originals first (old ones kept), then the thumbnails.
        shutil.rmtree(staged_full, ignore_errors=True)
        shutil.copytree(src, staged_full)
        if full.exists():
            stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
            full.rename(full_base / f"{name}.replaced-{stamp}")
            print(f"kept the previous originals as {name}.replaced-{stamp}")
        staged_full.rename(full)
        shutil.rmtree(staged_thumbs, ignore_errors=True)
        shutil.copytree(thumbs, staged_thumbs)
        if folder.exists():
            shutil.rmtree(folder)
        staged_thumbs.rename(folder)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        shutil.rmtree(staged_full, ignore_errors=True)
        shutil.rmtree(staged_thumbs, ignore_errors=True)
    added = new_bytes - old_bytes
    print(f"added {name}: {len(images)} image(s), {new_bytes} bytes in the tree, {added:+d} bytes against the "
          f"previous version" + (" (whose thumbnails stay in git history)" if old_bytes else "")
          + f"; originals in {full}")
    if not a.no_index:
        write_index()


# ------------------------------------------------------------------------------------------------
# drop, dupes

def cmd_drop(a):
    folder = GALLERY / a.entry
    if not ENTRY_NAME.fullmatch(a.entry) or not inside(folder, GALLERY) or not (folder / "entry.json").is_file():
        fail(f"no entry {a.entry!r} in docs/gallery")
    entry = json.loads((folder / "entry.json").read_text(encoding="utf-8"))
    names = {im["file"] for im in entry["images"]}
    unknown = [f for f in a.files if f not in names]
    if unknown:
        fail(f"{a.entry} has no images {unknown}; it has {sorted(names)}")
    keep = [im for im in entry["images"] if im["file"] not in a.files]
    if not keep:
        fail("an entry keeps at least one image")
    before = tree_bytes(folder)
    entry["images"] = keep
    if a.note:
        entry["note"] = a.note
    (folder / "entry.json").write_text(json.dumps(entry, indent=2, ensure_ascii=True) + "\n", encoding="utf-8",
                                       newline="\n")
    for f in a.files:
        (folder / f).unlink()
    print(f"dropped {len(a.files)} image(s) from {a.entry}: {tree_bytes(folder) - before:+d} bytes in the tree "
          f"(originals untouched)")
    write_index()


def cmd_dupes(a):
    exe = thumb_exe(a.thumb)
    entries = load_entries()
    n = 0
    for e in sorted(entries, key=sort_key):
        for f, d, g in earlier_duplicates(exe, e, GALLERY / e["_dir"], entries):
            print(f"{e['_dir']}/{f} repeats {d}/{g}")
            n += 1
    print(f"{n} repeated thumbnail(s)")


# ------------------------------------------------------------------------------------------------
# index

def esc(text):
    return text.replace("|", "\\|").replace("<", "&lt;").replace(">", "&gt;")


def meta_line(e):
    bits = []
    if e.get("milestone"):
        bits.append(e["milestone"])
    bits.append(e["topic"])
    if e.get("adapter") and e["adapter"] != "n/a":
        bits.append(f"{e['adapter']} GPU" if e["adapter"] in ("integrated", "discrete") else e["adapter"])
    if e.get("commit"):
        bits.append(f"commit `{e['commit']}`")
    if e.get("source"):
        bits.append(f"from `{e['source']}`")
    when = e["date"] + (f" {e['time']}" if e.get("time") else "")
    if e.get("approximate"):
        when = f"about {when}"
    if e.get("date_source"):
        when += f" ({e['date_source']})"
    return f"*{when}* · " + " · ".join(bits)


def img_tag(e, im, width):
    alt = esc(im.get("caption") or e["title"])
    return f'<a href="{e["_dir"]}/{im["file"]}"><img src="{e["_dir"]}/{im["file"]}" width="{width}" alt="{alt}"></a>'


def render_index(entries):
    entries = sorted(entries, key=sort_key)
    total = tree_bytes(GALLERY)
    dates = [e["date"] for e in entries]
    out = []
    w = out.append
    w("# Visual history")
    w("")
    w("Screenshots of Specula's tests, assets and games, dated, so the project's progress can be reviewed")
    w("over time: from the first grey test shapes to the refined scenes. Each entry is a folder with small")
    w("thumbnails and an `entry.json` (date, milestone, commit, what is shown and what changed, the script that")
    w("made it). Full-size originals stay on the machine that took them, in `gallery-full/` of the main")
    w("working tree (gitignored).")
    w("")
    w("This page is generated by `python scripts/gallery.py index`; don't edit it by hand.")
    w("")
    if entries:
        w(f"**{len(entries)} entries**, {sum(len(e['images']) for e in entries)} images, {dates[0]} to {dates[-1]}, "
          f"{total / 1024 / 1024:.1f} MiB in this folder (budget {TOTAL_BUDGET // 1024 // 1024} MiB).")
        w("")
    w("- [By subject](#by-subject)")
    w("- [Newest first](#newest-first)")
    w("- [Gallery: adding entries](#gallery-adding-entries)")
    w("")
    w("## By subject")
    w("")
    w("Each subject from its first capture to its latest, oldest first.")
    w("")
    for key, (heading, desc) in SUBJECTS.items():
        group = [e for e in entries if e["subject"] == key]
        if not group:
            continue
        w(f"### {heading}")
        w("")
        w(desc)
        w("")
        w("| When | Shown | What changed |")
        w("|---|---|---|")
        for e in group:
            when = ("about " if e.get("approximate") else "") + e["date"]
            lead = e["images"][0]
            more = f" (+{len(e['images']) - 1})" if len(e["images"]) > 1 else ""
            w(f"| {when}<br>{esc(e.get('milestone') or '')} | {img_tag(e, lead, 240)} | "
              f"**[{esc(e['title'])}]({e['_dir']}/entry.json)**{more}: {esc(e['note'])} |")
        w("")
    w("## Newest first")
    w("")
    for e in reversed(entries):
        w(f"### {esc(e['title'])}")
        w("")
        w(meta_line(e) + f" · [entry]({e['_dir']}/entry.json)")
        w("")
        w(esc(e["note"]))
        w("")
        w("<p>")
        for im in e["images"]:
            w(img_tag(e, im, 200 if len(e["images"]) > 2 else 320))
        w("</p>")
        captions = [f"{i}. {esc(im['caption'])}" for i, im in enumerate(e["images"], 1) if im.get("caption")]
        if captions:
            w("")
            w("<sub>" + " ".join(captions) + "</sub>")
        w("")
    w("## Gallery: adding entries")
    w("")
    w("After any live run (window check, playtest, asset or animation check, a new scene), add a few")
    w("representative captures, so the history keeps growing as the engine and games improve:")
    w("")
    w("```")
    w("python scripts/gallery.py add --date 2026-10-18 --subject sepulcra --topic game \\")
    w('    --title "Sepulcra: boss wave" --note "What is shown; what changed since the last entry." \\')
    w("    --milestone M3 --commit <sha> --adapter integrated --source scripts/sepulcra_play.py \\")
    w('    path/to/05-boss.png::"Boss wave" path/to/06-end.png::"The end"')
    w("```")
    w("")
    w("- **Pick, don't dump.** Two to six frames per entry: the ones that show what changed. For repeated runs")
    w("  of one script keep the first, the latest and the visibly different ones.")
    w("- **Look first.** Open every image before adding it; leave out black or broken frames, or say in the")
    w("  note that the entry shows a failure.")
    w("- **Honest dates.** Use the date the capture was made (commit date, file time or the run id in the")
    w("  capture folder, which is a Unix time). Pass `--approximate` and a `--date-source` when unsure.")
    w("- **Notes say what changed** since the subject's previous entry, in a sentence or two. `add` warns")
    w("  when a thumbnail repeats an earlier entry of the same subject: leave it out and write")
    w("  \"unchanged since <entry>\" instead (`python scripts/gallery.py dupes` lists repeats).")
    w("- **Committed entries are history.** Don't rebuild them: `drop ENTRY FILE...` removes thumbnails, and")
    w("  `add --force-replace` (which keeps the old originals) is for mistakes caught before a commit, since")
    w("  replaced thumbnails stay in git history.")
    w(f"- **Budgets:** {IMAGE_BUDGET // 1024} KiB per image, {ENTRY_BUDGET // 1024} KiB per entry, "
      f"{TOTAL_BUDGET // 1024 // 1024} MiB for the gallery. `python scripts/gallery.py check` and")
    w("  `crates/specula/tests/gallery.rs` enforce them. Thumbnails are JPEG (`--png` for pixel art).")
    w("- Subjects and topics are listed by `python scripts/gallery.py subjects`; add a subject to")
    w("  `SUBJECTS` in the script when a new kind of work starts.")
    w("")
    return "\n".join(out)


def write_index():
    entries = load_entries()
    (GALLERY / "README.md").write_text(render_index(entries), encoding="utf-8", newline="\n")
    print(f"wrote docs/gallery/README.md ({len(entries)} entries)")


def cmd_index(_a):
    write_index()


# ------------------------------------------------------------------------------------------------
# check

def cmd_check(_a):
    problems = []
    entries = load_entries()
    if GALLERY.is_dir():
        for p in sorted(GALLERY.iterdir()):
            if p.name.startswith("."):
                problems.append(f"{p.name}: a hidden file or folder (a staging leftover from an interrupted add? remove it)")
            elif p.is_file() and p.name != "README.md":
                problems.append(f"{p.name}: only README.md and entry folders belong at the top of docs/gallery")
            elif p.is_dir() and not (p / "entry.json").is_file():
                problems.append(f"{p.name}/: a folder without entry.json (an interrupted add? remove it)")
    for e in entries:
        d = GALLERY / e["_dir"]
        missing = [k for k in ENTRY_KEYS if k not in e]
        if missing:
            problems.append(f"{e['_dir']}: entry.json lacks {missing}")
        if not valid_date(e.get("date")):
            problems.append(f"{e['_dir']}: date {e.get('date')!r} is not YYYY-MM-DD")
        if not ENTRY_NAME.fullmatch(e["_dir"]) or not e["_dir"].startswith(f"{e.get('date')}-"):
            problems.append(f"{e['_dir']}: folder must be <date>-<slug> with its own date {e.get('date')}")
        if e.get("subject") not in SUBJECTS:
            problems.append(f"{e['_dir']}: unknown subject {e.get('subject')!r}")
        if not e.get("images"):
            problems.append(f"{e['_dir']}: no images")
        listed = {im["file"] for im in e.get("images", [])}
        on_disk = {p.name for p in d.iterdir() if p.name != "entry.json"}
        if listed != on_disk:
            problems.append(f"{e['_dir']}: files on disk {sorted(on_disk - listed)} not listed, listed {sorted(listed - on_disk)} missing")
        for p in d.iterdir():
            if p.is_dir():
                problems.append(f"{e['_dir']}/{p.name}/: entries hold no folders")
            elif p.name != "entry.json" and p.stat().st_size > IMAGE_BUDGET:
                problems.append(f"{e['_dir']}/{p.name}: {p.stat().st_size} bytes > {IMAGE_BUDGET}")
        size = tree_bytes(d)
        if size > ENTRY_BUDGET:
            problems.append(f"{e['_dir']}: {size} bytes > {ENTRY_BUDGET}")
    grand = tree_bytes(GALLERY)
    if grand > TOTAL_BUDGET:
        problems.append(f"gallery: {grand} bytes > {TOTAL_BUDGET}")
    readme = GALLERY / "README.md"
    # Compare ignoring line endings (a Windows checkout may have CRLF).
    if not readme.exists() or readme.read_text(encoding="utf-8").replace("\r\n", "\n") != render_index(entries):
        problems.append("docs/gallery/README.md is out of date: run python scripts/gallery.py index")
    for p in problems:
        print("problem:", p)
    dates = sorted(e["date"] for e in entries)
    print(f"{len(entries)} entries, {sum(len(e['images']) for e in entries)} images, {grand} bytes in docs/gallery"
          + (f", {dates[0]} to {dates[-1]}" if dates else ""))
    sys.exit(1 if problems else 0)


def cmd_subjects(_a):
    for k, (h, d) in SUBJECTS.items():
        print(f"{k:14s} {h}: {d}")
    print("topics:", ", ".join(TOPICS))
    print("adapters:", ", ".join(ADAPTERS))
    print("originals go to:", full_root())


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    add = sub.add_parser("add", help="add an entry from capture files")
    add.add_argument("--date", required=True, help="YYYY-MM-DD the captures were made")
    add.add_argument("--time", default="", help="HH:MM (local), orders entries within a day")
    add.add_argument("--date-source", default="", help="where the date comes from: commit, file time, run id, ...")
    add.add_argument("--approximate", action="store_true", help="the date is a best estimate")
    add.add_argument("--subject", required=True, help="see the subjects command")
    add.add_argument("--topic", required=True, help="see the subjects command")
    add.add_argument("--title", required=True)
    add.add_argument("--note", required=True, help="what is shown and what changed since the subject's last entry")
    add.add_argument("--milestone", default="")
    add.add_argument("--commit", default="")
    add.add_argument("--adapter", default="")
    add.add_argument("--source", default="", help="the script or tool that made the captures")
    add.add_argument("--slug", default="", help="folder name after the date (default: from the title)")
    add.add_argument("--force-replace", action="store_true",
                     help="rebuild an existing entry (old thumbnails stay in git history; old originals are kept)")
    add.add_argument("--width", type=int, default=WIDTH)
    add.add_argument("--quality", type=int, default=82)
    add.add_argument("--png", action="store_true", help="PNG thumbnails (pixel art, text)")
    add.add_argument("--thumb", default="", help="path to the thumb example binary")
    add.add_argument("--no-index", action="store_true", help="don't regenerate README.md (batch adds; run index after)")
    add.add_argument("files", nargs="+", help="capture files, each optionally PATH::caption")
    drop = sub.add_parser("drop", help="remove thumbnails from an entry (its originals stay)")
    drop.add_argument("entry", help="the entry folder name, e.g. 2026-10-18-sepulcra-now")
    drop.add_argument("files", nargs="+", help="thumbnail file names in that entry")
    drop.add_argument("--note", default="", help="a new note for the entry (say what was left out and why)")
    dupes = sub.add_parser("dupes", help="list thumbnails that repeat an earlier entry of the same subject")
    dupes.add_argument("--thumb", default="", help="path to the thumb example binary")
    sub.add_parser("index", help="regenerate docs/gallery/README.md")
    sub.add_parser("check", help="check budgets, listed files, orphans and the README")
    sub.add_parser("subjects", help="list subjects, topics and adapters")
    a = ap.parse_args()
    cmds = {"add": cmd_add, "drop": cmd_drop, "dupes": cmd_dupes, "index": cmd_index, "check": cmd_check,
            "subjects": cmd_subjects}
    try:
        cmds[a.cmd](a)
    except ThumbError as e:
        fail(str(e))


if __name__ == "__main__":
    main()
