# Packaging a non-Steam Bevy demo

**Goal:** produce a folder (or zip) someone can run without `cargo run` — binary **plus** `assets/`.

Steam upload is **later** ([ROADMAP.md](ROADMAP.md) G5). This path is enough for friends, playtests, and dogfood.

## One-command (from monorepo root)

```bash
# Package in-repo dogfood 2D demo
./scripts/package-demo.sh demo_2d games/demo-2d

# Package 3D dogfood
./scripts/package-demo.sh demo_3d games/demo-3d

# Custom output directory
./scripts/package-demo.sh demo_2d games/demo-2d --out /path/to/dist-out
```

What it does:

1. `cargo build --release -p <package>` (or `--manifest-path` for external kits)  
2. Copies the release binary into `dist/<package>/`  
3. Copies `assets/` next to the binary  
4. Optionally zips to `dist/<package>-<os>.zip`  

## Manual steps

```bash
cargo build --release -p demo_2d
mkdir -p dist/demo_2d
cp target/release/demo_2d dist/demo_2d/   # .exe on Windows
cp -R games/demo-2d/assets dist/demo_2d/
cd dist/demo_2d && ./demo_2d
```

**Important:** run the binary with **cwd** = the folder that contains `assets/`.

## Agent guidance

- Skill: **`bevy-package`**  
- MCP: `bevy_workflow` goal **`package_demo`**, prompt **`package_demo`**  
- Do not package until [GAME_DOD.md](GAME_DOD.md) playability is met  

## Related

- [SHIPPING.md](SHIPPING.md) — broader ship notes  
- [ROADMAP.md](ROADMAP.md) — G4 package, G5 Steam  
