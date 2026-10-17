# Agent-native game making: research for M3 and M4

Written 2026-10-17 by Claude (Opus 5.5) at the owner's request: "work out the best way that we can get this sorted out
for M3 and M4 [...] I want this engine and tool to help you make games easily. Not just for myself but for others."
The owner's direction (ROADMAP, 2026-10-17): agents build games through data and tools rather than hand-written code,
taste is engineered in, and the engine assists humans *and* agents. Sources are listed at the end; most are 2024–2026
papers and primary docs. Vendor blogs are marked where used.

## 1. What the evidence says

### Agents do best with text, structure and verifiable feedback
- **GameDevBench** (CMU, ICML 2026) is the first benchmark of agents doing game development: 333 Godot 4 tasks taken
  from tutorials, checked deterministically.
  - Gameplay logic tasks pass at ~51%; 2D graphics and animation at ~33%, and UI at ~32%.
  - Visual feedback (editor screenshots, gameplay video) helps most models. GPT-5.4 went from 41% to 52%, at up to 3.3x
    the cost. It is not reliable, though: one harness dropped from 45% to 30% with feedback.
  - The common failures are multimodal and structural: "agents frequently pick the wrong images or sprites" (walk
    frames instead of attack frames), and they "add nodes to incorrect levels in the tree, drop necessary signals, or
    assign resources to the wrong elements."
- **Practitioner and vendor reports** (treat with caution) agree on the shape of the problem:
  - text-based, code-first engines suit agents best; binary assets can't be diffed or repaired;
  - visual editors (Blueprints, material graphs) are out of reach for agents.
- **Unity AI and Roblox Assistant** both ship agentic loops now. Users report the same pains:
  - they're slow;
  - asset generation is good but world generation stalls;
  - changes are opaque or irreversible;
  - "execute code" requests come without saying what the code does.

**For Specula:** our foundations already match what works: everything is text, changes are journaled and undoable,
and verification comes back as data. The weak spots for agents are picking the right visual asset and wiring
structure correctly. Both improve when the engine gives names, schemas and checks instead of pictures alone.

### Agents are bad at raw coordinates and good at relations
- LLMs make "systematic coordinate errors" in 3D placement: wrong heights, off-by-one stacking, duplicates, and
  neglected axes in Minecraft-style building.
- Letting the model plan only in 2D while a deterministic executor computes height raised accuracy to 94.6%. That beat
  larger models working directly in 3D (90.3%) and the previous best system (76.3%).
- **Holodeck** (AI2, CVPR 2024) has GPT-4 propose *spatial relations* between objects ("next to", "facing", "against
  the wall"). A constraint optimiser then places everything, and humans preferred its scenes to procedural baselines.
- In rearrangement benchmarks, the largest error bucket is placements that satisfy the relation but are physically
  invalid (collisions, ~26%).

**For Specula:** agents should state *relations and intent*, and the engine solves positions deterministically and
reports violations as data. This is the same principle that made the ID buffer valuable: the engine computes, the
agent decides.

### Models can't judge beauty alone, and comparisons beat scores
- **Visual Aesthetic Benchmark** (ICML 2026): the task is to pick the best and worst image from sets with matched
  subjects. The best of 20 frontier models plus six reward models got both right in **26.5%** of tasks; human experts
  scored **68.9%**. Models were unstable when the image order changed.
  - Fine-tuning a 35B model on **2,000 expert comparisons** brought it close to a 397B model, so preference data
    transfers.
  - Even for experts, per-image scores "align poorly" with their own direct comparisons. Ranking is more consistent.
- Other 2026 work agrees: relative comparisons beat absolute scores against human labels (correlation 0.74 vs 0.68),
  and small pairwise VLM judges beat larger pointwise reward models. VLMs still miss small deformations.

**For Specula:** never ask a model "is this good?" in the absolute. The engine should **measure** what can be measured
(contrast, palette, silhouette, scale, budgets), **compare** variants, and **ask a human to pick** where taste
matters. Every human pick is valuable training data for the project's style.

### Humans communicate taste by pointing, not describing
- **Design Galleries** (Marks et al., SIGGRAPH 1997) is the classic answer to "parameter tweaking is painful": the
  computer generates a broad, perceptually *dispersed* set of variants and arranges them for browsing, and the user
  picks. It was used for lighting, volume rendering and animation.
- **Sequential Gallery** (Koyama et al., 2020) adds preferential Bayesian optimisation. The user picks the best from
  a small gallery each round, and the system searches the parameter space from those picks. It finds good settings
  for many-parameter designs in a few rounds and "drastically outperforms" the earlier one-slider-at-a-time method.
- **Midjourney**, the most popular image tool, moved users away from prose:
  - style references (images or codes);
  - moodboards (curated image sets);
  - personalisation codes built from "this-or-that" ratings.

  Practitioners report they rely on those "rather than detailed descriptions."
- **Interactive evolution** research warns that human picking causes fatigue and inconsistency, and recommends
  learning a model from a user's past picks so later generations need fewer of them.

**For Specula:** "choose, don't describe" is well established. The engine needs a gallery (variants spread across the
parameter space), a pick-driven optimiser, and a **preference store** that remembers picks across sessions.

### Gameplay as data is feasible, but almost nobody does it
- A 2025 **declarative ECS** model (SBGames) ran *ten complete games across genres*, from physics puzzles to visual
  novels, on a reference engine with no game-specific code. It notes that game logic in mainstream engines "remains in
  arbitrary code."
- **VGDL, Ludii and PuzzleScript** are game description languages.
  - LLMs generate valid games in them when the output is **grammar-constrained** or iterated against **compiler
    errors**.
  - Inferring rules from traces through a two-stage causal model beat direct code generation (up to 81% preference).
- **Unreal's Gameplay Ability System** proves that data-driven abilities and effects scale to AAA: attributes, effects
  as data, and tags.

**For Specula:** a typed rules language with a schema, a validator that returns actionable errors, and a
compile-then-feedback loop is the path to "agents build games without hand-writing code." Code remains the escape
hatch.

### Balance by simulation works, but heuristics mislead
- **EA** ran agents against a model of *The Sims Mobile*; the imbalances they found changed the shipped game.
- **RuleSmith** (2026) combines LLM self-play with Bayesian optimisation over rule parameters, converging on balanced
  configurations with "interpretable rule adjustments."
- But simulated balance depends entirely on the heuristic, and agents don't play like humans. Human playtests must
  confirm it.

**For Specula:** our determinism makes this cheap (thousands of headless runs). Pair it with designer *targets* and
an optimiser, and flag results for human playtests.

### Animation: procedural motion suits agents; foot sliding is the standard metric
- **Rain World's** creatures are point-mass bodies with procedural limbs and no keyframes. That is code and data an
  agent can author.
- Motion matching can't create new poses, so its database sets the quality.
- The common quality metric is **foot sliding**: foot velocity while the foot is within ~2.5 cm of the ground. It is
  paired with contact checks, penetration and positional error. Foot IK is the usual fix.

**For Specula:** agents can author gait-driven procedural motion, and the engine can measure foot sliding, contact,
penetration and pops as data.

### Tool design for agents
Anthropic's engineering guidance on tools for agents:
- consolidate around workflows rather than raw API calls;
- namespace tool names;
- return high-signal, human-readable context instead of opaque ids;
- offer concise and detailed response formats;
- paginate and truncate by default with guidance;
- write actionable error messages;
- **evaluate tools with agents** on realistic, verifiable multi-step tasks, studying transcripts (including what
  agents leave out).

Specula follows most of this already (bounded responses, structured errors). Two things are missing: response
formats, and **an agent evaluation suite** for our own tools.

## 2. Principles for Specula, derived from the above

1. **Data with a grammar, not free code.** Gameplay, animation graphs, UI, style: typed schemas, validated on every
   edit, with errors that say what to change. Code modules only where no building block fits.
2. **Relations, not coordinates.** Agents state intent ("along the north wall, 2 m apart, facing the path"); a
   deterministic solver places things and reports violations.
3. **Measure, compare, then ask.** Engine metrics first; comparisons between variants second; a human pick for taste.
   Never an absolute "is this good?" from a model.
4. **Humans pick, the engine remembers.** Every pick is stored per project, becomes a constraint or prior, and
   compounds.
5. **Verify everything as data:** placement, physics, animation quality, balance, style. That is the sight principle
   extended to every system.
6. **Simulate at scale, confirm with people.** Headless sweeps and optimisers propose; human playtests confirm.
7. **Tools are a product.** Designed for agents, evaluated by agents, with a regression suite.

## 3. Proposals for M3 (logic, physics, animation and time)

- **Lex, the gameplay language** (Latin *lex*, "law"). Typed text data for behaviours, attributes, effects (GAS-like),
  events, triggers, state machines, timers, spawners, economies, and win/lose conditions.
  - A schema plus a validator with actionable errors, and `lex_*` tools to read, edit, validate and simulate.
  - Edits are journaled, claimable and undoable like the world.
  - *Proof:* Sepulcra's rules rebuilt in Lex through tools alone, with replay hashes matching the Rust version.
- **Locus, the placement solver.** A `world_place` tool takes relations (on, against, along, between, facing,
  spacing, avoid, inside region, on navmesh) and solves them deterministically. Unsatisfied relations come back as
  data with the nearest valid alternative.
- **Animation**, beyond the A3/A4 items already in the roadmap:
  - procedural motion generators (gait from a rig, secondary motion, recoil, breathing) as data;
  - **motion checks** as packet data (foot sliding with the 2.5 cm contact rule, ground penetration, joint limits,
    transition pops, root drift), with thresholds in `style.toml` so failures are errors.
- **Physics checks as data**: playtest assertions on contacts, tunnelling, rest states and energy, plus sight of
  colliders and contacts in captures.
- **Balance lab**:
  - designer *targets* as data (win rate per wave, gold curve, time to kill);
  - headless sweeps across parallel engines;
  - Bayesian optimisation over Lex parameters;
  - results as data, with "needs a human playtest" flags.
- **Specula agent evals**: a fixed set of realistic tasks, each with a verifiable outcome. Examples: build a room that
  passes these checks; add an enemy that patrols; make wave 3 harder by 10% without breaking wave 4. It runs against
  the tools after every change, like a test suite for tool quality. It is also our answer to GameDevBench-style
  claims when marketing.
- **Tool ergonomics:** a `response_format` (concise/detailed) on the heavier tools, and an audit of tool count and
  naming per the Anthropic guidance.

## 4. Proposals for M4 (asset studio, style and review)

- **Galleries, "choose, don't describe".** The engine generates variants spread across a parameter space (Design
  Galleries' dispersion: lighting, palette, proportions, materials, layout), shows them side by side in the window or
  editor, and the human picks, or clicks the part they dislike. Agents get the same gallery as data (`style_gallery`).
- **Pick-driven tuning.** Several rounds of picks drive preferential Bayesian optimisation (Sequential Gallery) over
  style parameters, so a few clicks reach a good setting in a many-parameter space.
- **Preference store.** Every pick and rejection, with its note, is stored per project, and optionally per user, as
  data.
  - Agents consult it before proposing.
  - Repeated picks become `style.toml` rules or priors.
  - Later, if we choose to: a small learned preference model (VAB shows ~2,000 comparisons transfer).
- **Reference boards.** The human drops in images they like. The engine measures them (value histogram, saturation,
  palette, contrast, edge density, shadow softness) and those numbers become targets the scene is checked against.
- **Taste checks as data**, reported like budget errors:
  - silhouette distinctness between sibling assets (rendered masks, overlap scores) and readability at thumbnail
    size;
  - value contrast between subject and background;
  - palette distance and neutral saturation (STYLE.md's < 25% rule);
  - scale consistency against reference figures;
  - flat-lighting detection.
- **Shot sets.** Standard camera rigs for every review: gameplay view, close-up, worst-case lighting, phone-sized,
  iGPU. No more judging from one flattering angle.
- **Style configurator and first-run onboarding** (owner's diary 2026-10-16). Agents ask new users how the game should
  look through UI options (galleries), starting from a base style. "Can't afford" and other UI colours are offered as
  alternatives in the same way.
- **Structured critique queue.** Sight-skeptic findings become items that humans and agents work through, and every
  item links its capture and metric.

## 5. Why this matters for marketing

The competition bolts agents onto human tools: slow assistants, opaque changes, binary assets. The research says what
agents actually need:
- text and schemas;
- relations solved by the engine;
- verification as data;
- comparison instead of judgement;
- humans who pick instead of prompt.

Specula can be the engine built that way from the start. It is efficient by default, deterministic, and has a
measurable agent benchmark to prove it. The pitch for others: **"describe what you want, pick what you like, and the
engine and its agents make it, checked."**

## 6. Risks and open questions

- **Lex scope:** too small and agents fall back to code; too big and it becomes a bad programming language. Start
  from Sepulcra and one contrasting genre (a small action game), and grow by evidence: every time the escape hatch is
  needed, record why.
- **Taste checks are proxies.** They catch failures, not greatness, so human picks stay in the loop.
- **Preference data** is personal. Per-project by default; sharing across projects is the owner's call.
- **Agent evals can be gamed** by tuning tools to the tasks. Keep a held-out set, per the Anthropic guidance.
- **Visual feedback costs tokens** (GameDevBench: up to 3.3x). Measurements first, images when needed.

## Sources

- GameDevBench: [arXiv 2602.11103](https://arxiv.org/html/2602.11103v2), [ICML 2026](https://icml.cc/virtual/2026/poster/64919)
- Visual Aesthetic Benchmark: [ICML 2026](https://icml.cc/virtual/2026/68543), [arXiv 2605.12684](https://papers.cool/arxiv/2605.12684)
- Relative vs absolute aesthetics: [arXiv 2606.05778](https://arxiv.org/html/2606.05778v3), [GenArena arXiv 2602.06013](https://arxiv.org/pdf/2602.06013), [VLM aesthetics arXiv 2606.00112](https://arxiv.org/html/2606.00112v1), [VLM vs human IQA arXiv 2603.24578](https://arxiv.org/html/2603.24578v1)
- Design Galleries: [Marks et al. 1997](https://vcg.seas.harvard.edu/publications/19970101-design-galleries-a-general-approach-to-setting-parameters-for-computer-graphics-and-animation)
- Sequential Gallery: [Koyama et al. arXiv 2005.04107](https://arxiv.org/pdf/2005.04107)
- Interactive evolutionary art: [Li et al. 2013](https://gpbib.cs.ucl.ac.uk/gp-html/Li_2013_GPEM.html), [Ekárt et al.](https://publications.aston.ac.uk/id/eprint/17157/1/Modelling_the_underlying_principles_of_human_aesthetic_preference_in_evolutionary_art.pdf)
- Midjourney: [Style Reference docs](https://docs.midjourney.com/docs/style-reference), [moodboards and personalisation](https://geekycuriosity.substack.com/p/midjourney-mixing-moodboard-style)
- Declarative ECS: [SBGames 2025](https://sol.sbc.org.br/index.php/sbgames/article/view/45426)
- Game description languages: [Ludii grammar generation arXiv 2407.17404](https://arxiv.org/pdf/2407.17404), [VGDL LLMGG arXiv 2404.08706](https://arxiv.org/html/2404.08706v2), [VGDL causal induction arXiv 2602.00190](https://arxiv.org/html/2602.00190v1), [grammar-aligned RL arXiv 2503.15783](https://arxiv.org/pdf/2503.15783)
- Unreal Gameplay Ability System: [Epic docs](https://dev.epicgames.com/documentation/en-us/unreal-engine/understanding-the-unreal-engine-gameplay-ability-system)
- Spatial reasoning: [2.5-D decomposition arXiv 2605.07066](https://arxiv.org/pdf/2605.07066), [Holodeck CVPR 2024](https://arxiv.org/html/2312.09067v2), [ReRef-3D arXiv 2608.16011](https://arxiv.org/pdf/2608.16011)
- Balance and playtesting: [Exploring Gameplay With AI Agents (EA)](https://ojs.aaai.org/index.php/AIIDE/article/view/13034), [RuleSmith arXiv 2602.06232](https://arxiv.org/abs/2602.06232v1)
- Animation: [Rain World procedural design](https://unity.com/en/blog/exploring-procedural-design-rain-world), [Learned motion matching (Ubisoft)](https://www.ubisoft.com/en-us/studio/laforge/news/6xXL85Q3bF2vEj76xmnmIu/introducing-learned-motion-matching), [foot-sliding metric arXiv 2209.11478](https://arxiv.org/pdf/2209.11478)
- Tools for agents: [Anthropic, Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)
- Current assistants (user reports): [Unity AI discussion](https://discussions.unity.com/t/unity-ai-my-take-the-good-the-bad-and-the-ugly/1721390), [Unity AI review](https://darkounity.com/blog/unity-ai-assistant-review-not-ready-for-real-work), [Roblox Assistant forum](https://devforum.roblox.com/t/the-ai-assistant-is-unbelievably-slow/4196052)
- Practitioner reports (vendor blogs, treat with caution): [AI coding tools for game dev](https://chierhu.medium.com/ai-coding-tools-for-video-game-development-a-first-principles-analysis-of-what-actually-works-90dfa10edd13), [MCP comparison](https://www.strayspark.studio/blog/unity-mcp-server-vs-unreal-godot-blender-2026), [Godot to Unity with agents](https://www.dan-malone.com/blog/ai-agents-game-godot-to-unity)
