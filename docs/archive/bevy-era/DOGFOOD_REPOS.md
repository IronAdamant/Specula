# Dogfood game repositories (sibling trees)

Crystal Drift and Iron Feud live **outside** the Specula monorepo so they can version assets/craft independently.

| Game | Path | Notes |
|------|------|--------|
| Crystal Drift (2D) | `/Users/aron/Documents/coding_projects/Crystal Drift` | Own `.git`; features `remote,capture` |
| Iron Feud (3D) | `/Users/aron/Documents/coding_projects/Iron Feud` | Own `.git`; `IRON_FEUD_AUTO_PLAY=1` for Playing |

Agent sight plans reference these absolute paths. Commit craft/asset work in each tree separately from Specula. Ignore `target/` and `captures/` locally unless intentionally snapshotting eyesight evidence.
