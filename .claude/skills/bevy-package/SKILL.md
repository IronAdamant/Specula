---
name: bevy-package
description: >
  Package a Bevy short demo for non-Steam distribution: release binary plus
  assets/ in a dist folder or zip. Use when packaging, making a playtest build,
  distributable demo, package_demo, or /bevy-package. Confirm GAME_DOD first.
metadata:
  short-description: "Zip/folder package: binary + assets"
---

# Package Bevy demo (non-Steam)

Produce a **runnable folder** (and optional zip). Steam is **later** — do not block on Steamworks.

## Prerequisites

1. Game meets **`docs/GAME_DOD.md`** (or is explicitly a package-only smoke).  
2. `assets/` exists next to the game crate.  
3. Package name matches the binary (`demo_2d`, etc.).

## Preferred command (monorepo)

```bash
./scripts/package-demo.sh demo_2d games/demo-2d
./scripts/package-demo.sh demo_3d games/demo-3d
# custom out:
./scripts/package-demo.sh demo_2d games/demo-2d --out /path/to/out
```

## Manual

```bash
cargo build --release -p <package>
mkdir -p dist/<package>
cp target/release/<package> dist/<package>/
cp -R <game_dir>/assets dist/<package>/
cd dist/<package> && ./<package>
```

**CWD must be the folder containing `assets/`.**

## Agent steps

1. Verify playability (captures / GAME_DOD).  
2. Run package script from repo root.  
3. Confirm `dist/<package>/` has binary + `assets/`.  
4. MCP: `bevy_workflow` goal `package_demo`.  

## Docs

- `docs/PACKAGING.md`  
- `docs/SHIPPING.md`  
