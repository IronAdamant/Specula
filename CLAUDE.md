@AGENTS.md

## Claude Code notes

- MCP server `specula` is registered in `.mcp.json` (needs `specula` on PATH: `cargo install --path crates/specula --force`).
- Skills: canonical source is `skills/`; `.claude/skills/` and `.grok/skills/` are generated copies. Edit `skills/`, then run `python scripts/sync_skills.py`.
- Before claiming sight results pass, delegate to the `sight-skeptic` subagent with the packet JSON and capture paths.
- Active plan: [docs/SPECULA_PLAN.md](docs/SPECULA_PLAN.md).
