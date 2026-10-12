# Choosing the first game

Status on 2026-10-13: the owner is choosing between a few game ideas. Pick the one that is **fast enough to work with**
on Specula. This is the first item of the next session.

The first game is the forcing function for the engine. Its needs decide which roadmap lanes grow first in Phase B
(see "Path to production" in [ROADMAP.md](ROADMAP.md)). A well-chosen first game saves years; a poorly chosen one pulls
in the most expensive systems first.

## What makes an idea fast on Specula

- **Stylised, shape-driven art.** Buildings, props, terrain and creatures made of clear shapes play to procedural
  modelling ([design/PROCEDURAL_MODELLING.md](design/PROCEDURAL_MODELLING.md)). Photoreal art and detailed human faces
  don't.
- **Few animated humanoids.** Every walking, fighting, emoting character needs skeletal animation, imported clips and
  blending ([design/ANIMATION.md](design/ANIMATION.md)). Creatures, vehicles, machines or a small cast are much cheaper.
- **Single-player.** Multiplayer comes after v1.0.
- **Simple physics.** Collision and a character controller (P1–P2) come early. Stacking, ragdolls and physics puzzles
  (P3–P4) come later ([design/PHYSICS.md](design/PHYSICS.md)).
- **Environments a swarm can split up.** Districts, islands, levels or rooms that agents can claim and build in parallel
  are where Specula is strongest.
- **Testable by replay.** Clear rules and outcomes (did the player reach X, did the wave end) suit headless playtest
  swarms ([design/DETERMINISM.md](design/DETERMINISM.md)).
- **Modest text.** Light dialogue in English first. Heavy branching narrative plus many languages adds a lot of UI,
  text and localisation work ([design/UI.md](design/UI.md)).

## What each kind of game needs from the engine

| Kind of game | Fit | Main engine work it pulls forward |
|---|---|---|
| Exploration / atmosphere (walk, discover, light puzzles) | Excellent | Character controller, lighting, procedural environments |
| Top-down action / roguelite with creatures | Very good | Navigation and AI, simple animation, particle effects |
| Tactics / turn-based strategy | Very good | UI, pathfinding, simple animation; deterministic by nature |
| City, colony or base builder | Good, and shows off swarms | UI, procedural buildings, agent AI, performance with many units |
| 2D platformer or puzzle game | Good | 2D rendering, 2D physics, sprite art pipeline |
| Third-person action with melee combat | Slower | Heavy animation, hit detection, camera work |
| Open world or online multiplayer | Not first | Streaming, netcode, huge content scope |

**Rule of thumb:** pick the idea the owner would still enjoy after a hundred playtests. Judge its scope by how short its
first fun 10 minutes can be. That 10–15 minute slice is the Phase B gate.

## Questions to ask about each idea

Claude asks these when the owner presents ideas (rough is fine):

1. **The core loop in one sentence.** What does the player do every 30 seconds? Every 10 minutes?
2. **2D or 3D?** Which camera: first-person, third-person, top-down, isometric or side-on?
3. **Who is on screen?** How many animated characters? Humanoid, creatures or machines? Does the player character
   need detailed animation?
4. **What does the world look like?** Art style references (games, films, art). How much of it could be procedural?
5. **What physics does play depend on?** Walking and collision only, or throwing, stacking and destruction?
6. **How much text?** Dialogue, quests, lore? Which languages at launch?
7. **Single-player?** Any online features at all?
8. **Size of the first slice.** What is the smallest version that's fun for 10–15 minutes?
9. **Where does it ship?** Windows + Steam Deck, or macOS too? Is Steam integration needed (achievements, cloud
   saves)?
10. **Why this one?** What excites the owner about it? Motivation over years matters as much as engine fit.

## How each idea gets scored

For each idea, Claude produces:
- **Fit score** against the criteria above (strong, medium or weak on each).
- **Lanes pulled forward:** which roadmap items Phase B must include, and roughly how big each is.
- **Risks:** the one or two features that could balloon, such as melee combat feel, crowds or branching dialogue.
- **First slice sketch:** what the 10–15 minute playable slice would contain.
- **Recommendation**, with reasons. **The owner decides.**

## Decisions this settles

These are also listed in ROADMAP.md under "Path to production":
- [ ] **First game chosen**, with genre, art style, slice scope and launch languages.
- [ ] **Steamworks:** may `steam_api64.dll` be loaded at runtime as an optional platform service?
- [ ] **v1.0 platforms:** Windows + Steam Deck, or macOS too?

## After choosing

1. Write the **game design doc** (core loop, slice contents, progression) in the game's own repo or folder.
2. Write the **style bible** with Claude: palette, shape language, proportions, material presets, references. This is
   where Claude's taste is meant to contribute.
3. Build **M1 persistence (save/load)** first, so greybox work survives restarts.
4. Greybox the first space live through MCP, then continue M1 with the game pulling features.
