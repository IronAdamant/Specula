#!/usr/bin/env python3
"""Mirror canonical skills/ into each agent's skill folder.

skills/ is the source of truth. Claude Code reads .claude/skills/, Grok Build
reads .grok/skills/; both use the same SKILL.md frontmatter, so they get exact
copies. Copies rather than symlinks: symlinks on Windows need Developer Mode.

    python scripts/sync_skills.py          # write mirrors
    python scripts/sync_skills.py --check  # exit 1 if a mirror is stale (CI)
"""

import filecmp
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "skills"
MIRRORS = [ROOT / ".claude" / "skills", ROOT / ".grok" / "skills"]


def stale(mirror: Path) -> list[str]:
    """Relative paths that differ between SOURCE and mirror (missing, extra, or changed)."""
    if not mirror.is_dir():
        return ["<missing mirror>"]
    out = []
    cmp = filecmp.dircmp(SOURCE, mirror)
    stack = [(cmp, Path())]
    while stack:
        c, rel = stack.pop()
        out += [str(rel / n) for n in c.left_only + c.right_only + c.funny_files]
        _, mismatch, errors = filecmp.cmpfiles(c.left, c.right, c.common_files, shallow=False)
        out += [str(rel / n) for n in mismatch + errors]
        stack += [(sub, rel / name) for name, sub in c.subdirs.items()]
    return out


def main() -> int:
    if not SOURCE.is_dir():
        print(f"no canonical skills folder at {SOURCE}", file=sys.stderr)
        return 1
    check = "--check" in sys.argv[1:]
    bad = False
    for mirror in MIRRORS:
        diffs = stale(mirror)
        label = mirror.relative_to(ROOT)
        if check:
            if diffs:
                bad = True
                print(f"{label}: stale ({', '.join(sorted(diffs)[:10])})")
            continue
        if diffs:
            shutil.rmtree(mirror, ignore_errors=True)
            shutil.copytree(SOURCE, mirror)
            print(f"{label}: synced")
        else:
            print(f"{label}: up to date")
    if bad:
        print("run: python scripts/sync_skills.py", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
