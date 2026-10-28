# Review queue, critique queue, merges and the director

Status: built 2026-10-22 (M4 lane: review queue, structured critique queue, conflict merges, the director pattern's M4
part). Code: `crates/specula/src/review.rs` (submissions and critiques as data, persistence), `review_tools.rs`
(arguments and schemas), `review_engine.rs` (decisions, promotion, the board links, the director tools),
`review_screen.rs` (the window's review screen), `merge.rs` (three-way merges), `director.rs` (design-doc sections
to plans). Proof: `scripts/review_check.py`.

## Why

A swarm multiplies output, not taste (HANDOFF: "the human stays the director"). Without a queue the owner reviews by
reading logs and screenshots in chat, one change at a time, and becomes the bottleneck (ROADMAP need #5, "cheap human
direction"). The queue makes the owner's judgement cheap to apply: finished work arrives **with its evidence**
(captures, checks, the journal entries it covers), is judged **in batches**, and the verdict acts by itself: an
approval moves the work into the game, a rejection becomes a task for the agent that made it. Reviewers' findings
(sight-skeptic, lane reviewers) stop being prose in a report and become items someone owns until they are closed.

## Submissions

`review_submit` puts finished work on the queue:

```json
{"title": "Crypt props, batch 2", "kind": "asset", "batch": "crypt_props",
 "summary": "12 props for the east crypt, T1 recipes, all under the triangle budget",
 "paths": ["/staging/crypt_props"], "promote": {"to": "/world/props"},
 "captures": [{"path": ".../orbit_0003.png", "kind": "contact_sheet", "packet": ".../orbit_0003.json"}],
 "checks": [{"kind": "style", "name": "style.toml", "passed": true, "detail": "12/12 within budgets"},
            {"kind": "taste", "name": "silhouette_distinct", "passed": false, "severity": "warning",
             "metric": "silhouette_iou", "value": 0.81, "limit": 0.75}],
 "journal_since": 412}
```

- **Kinds**: `asset`, `lex`, `scene`, `ui`, `changes` (any journaled change set). The kind is a label for listings
  and the screen; what a submission *does* on approval is its `promote` (below), so any kind can promote or not.
- **What it covers**: entity subtrees (`paths`; their versions are recorded, a stamp over every entity below them),
  journal entries (`journal: [ids]`, or `journal_since: id` for the submitter's own entries after it, at most 256),
  and the task it is for (`task`, default the submitter's current task on the board). Journal entries must be the
  submitter's own (`not_your_entry`), and an entry another pending or approved submission covers is never covered
  twice (`already_covered`; `journal_since` skips it), so a rejection's revert can't reach anyone else's work.
- **Evidence**: captures are files the engine wrote (`sight_frame`, `sight_orbit`, `sight_compare`, `anim_filmstrip`,
  the asset studio's contact sheets): each must exist and be a PNG; its packet (JSON) is optional. Checks are data
  from the tools that ran them (`anim_check`, `physics_check`, `style_check`, `taste_check`): kind, name, passed,
  severity, and a metric with value and limit. The engine adds its own checks at submit (`paths_exist`,
  `captures_exist`) and at decision time (`unchanged_since_submit`).
- **Bounds**: 8 captures, 64 checks, 64 paths, 256 journal ids, 10,000 entities under the paths, a title of 200 and
  a summary of 2,000 characters; 4,096 submissions kept (the oldest decided ones make room), 16 MiB of text for the
  queue and critiques together.
- **Resubmitting**: `supersedes: id` withdraws the older pending submission (or links the rejected one), so the
  queue never shows two versions of the same work.

## Decisions

`review_decide` approves or rejects **in batches**: `{ids: [...] | batch: "crypt_props", verdict, note}` or a list of
`decisions` with their own verdicts and notes. Who may decide: an agent whose `agent_hello` role is `owner`,
`reviewer`, `lead` or `director`, and the human at the window. Nobody decides their own submission (`self_review`).
The submitter may `withdraw` its own.

- **Stale work is refused.** If anything under the submission's paths changed since it was submitted, the captures no
  longer show what would be approved: `stale_submission` (with what changed), unless `force: true`.
- **Approve → promote.** A submission with `promote: {to, replace?, tag?}` moves its subtrees under `to` in one
  journaled batch written *as the decider* (a `Rule::World` commit through the write policy, docs/design/WRITE_POLICY.md), so it is claim-checked like any write: a claim on the target refuses it
  and nothing moves (the answer says who holds it). `replace: true` deletes a same-named entity already there (in
  the same batch); `remove_tag` (default `"staging"`) is taken off the moved roots. Without `promote`, approval
  records the verdict (the work is already where it belongs, for example a Lex edit to the live set). Each submission
  is all or nothing; a batch reports each one, and one refused submission doesn't stop the others.
- **Reject → task.** The note is required. The rejection posts a task on the board assigned to the submitter
  (`Rework: <title>`, the note and the open critiques as detail, the submission's paths and entities as links, tags
  `review`, `rework`, `submission:<id>`, parented to the submission's task). `revert: true` also undoes the
  submission's journal entries, for work that must not stay in the world: the write policy's undo
  (`Live::undo`) as the decider, claim-checked step by step. It only ever takes back this submission's own work: the
  queue refuses first when any later journal entry outside the submission, by anyone including the submitter, touched
  the same entities (`revert_conflict`), and when an approved submission covers one of the entries or the entities
  sit under its roots (`revert_overlaps_approved`). With those refused, the undo's `force` only lets the decider undo
  entries that aren't theirs.
- **The board hears about it**: the submission's task gets `review` events (submitted, approved, rejected), so
  `task_get` and `world_rollup` show where work stands.
- **Persisted with the board**: `review.json` next to `board.json` in the world's folder, written whenever the board
  is, read back with the world. Owners and deciders come back by name, like the board's.

## The human view

The window has a review screen (`specula-ui`, an engine screen drawn over the game): it pages through pending
submissions and shows each one's first capture, title, submitter, kind, batch, checks (failed ones first), open
critiques and summary, with **Approve**, **Reject**, **Approve batch**, **Prev**, **Next** and **Close**. Reject asks
for a reason from a short list (off-style, scale, needs work, broken), so it works without a keyboard; the note says
the reason and that it came from the window.

- **Every input**: mouse and touch (the buttons; a "Review (n)" tab appears at the screen's bottom-left while work
  is pending), keyboard (F9 opens and closes it, arrows and Enter move and press, PageUp / PageDown page), gamepad
  (focus navigation, A to press, B to close, LB / RB to page; LB + RB + Y opens and closes it). F5: humans are never
  locked out; F6: nothing needs a keyboard.
- **Modal while open**: the hosted game, modules and live Lex runs get no window input then (a click on Approve never
  also places a stone). Gamepads too: the engine polls them and hands them to the game in `Frame::pads`
  (Sepulcra reads them there, not from its own poll), all released while the screen is open, and a button held then
  stays released until it is let go (`game::PadGate`). Sepulcra's `tests/modal_pads.rs` proves a pad press on the
  open screen leaves `play_state` unchanged, while the same press with the screen closed calls the next wave.
  The key, button or pad press that closes the screen doesn't reach the game either; a press on the tab is the
  screen's, not the game's.
- **Nothing is decided unseen.** A decision press counts only when the shown submission has been on screen for
  0.4 s (counted from when the screen opens, too) and its capture is drawn (or failed to load); "Approve batch (n)"
  approves exactly the n it counted, and only once that count has held for 0.4 s; after a decision focus moves to Next, so a double press or a
  double click never approves the submission that just appeared, and neither does a change under the pointer.
- **Agents can drive it too** (`review_screen {open, show, next, prev, close}`, role owner, reviewer, lead or
  director; anyone may ask its status), so a lead agent can put a batch in front of the owner, and captures with
  `include_ui` show it. After the owner closes it, agents can't reopen it for 30 s (`owner_closed`): humans are
  never locked out, nor locked in. Decisions made in the window are attributed to `window:owner`.
- The capture is decoded on one worker thread at a time (paging fast only changes what is decoded next), refused
  above 4096 x 4096 pixels from its PNG header, scaled to at most 1024 px and shown fitted in its panel; at most one
  texture is held.

## Structured critiques

`critique_add` records findings as items (at most 32 per call): `{text, severity, source, capture?, packet?,
metric? {name, value, limit}, entity? | path?, submission?}`. `source` is who found it (`sight-skeptic`,
`lane-reviewer`, an agent or the owner). Items are worked through with `critique_update {id | ids, action}`:

- `assign` (to an agent), `resolve` (with a note), `dismiss` (a reason is required), `reopen`;
- `to_task`: posts a board task for it (assigned if given, linked to its entity or path, tagged `critique`) and
  marks it `in_task`. When that task is done, the critique resolves itself with the task's summary; when it is
  cancelled, the critique is open again.

`critique_list` pages with filters (status, severity, source, submission, path prefix, assignee). Critiques on a
submission show on the review screen and go into a rejection's task.

## Conflict merges (M4 part of conflict help)

M3's conflict help said what changed (`base`, `theirs`, `yours`). M4 adds a **suggested three-way merge** where the
engine knows the data's shape, offered on the refusal itself:

- **`world_set` / `world_batch` `stale_version`**: for `props` keys both sides changed, the merge of base, theirs
  and yours (the entity's game data is JSON). The refusal's `conflict.merge` holds `clean`, `conflicts` and `apply`:
  a ready `world_set` call (the merged props, the other fields you set, `expect_version` = the current version).
  Send it as is to accept, or adjust it. Your other fields are compared as the entity listing shows them (a colour
  as hex, a parent given by id as its path), so setting what they set is never a conflict. For op `i` of a
  `world_batch`, `merge.next` says to put the merged op back into the batch.
- **`lex_edit` `version_conflict`**: the item at the version you read (from the journal), the item now, and your op
  (a put, or a patch applied to the base) are merged; `apply` is a `lex_edit` put of the merged item at the current
  version.
- **`merge_suggest {kind, base, theirs, yours}`** for anything else whose three versions an agent holds: recipes,
  UI screens, style files and project files (until the project-data lane gives files versions of their own, when its
  refusals can call the same merge).

The merge is structural: objects key by key; arrays of objects matched by identity (`id`, then `name`, then the
kind's own keys: Lex `behaviours`, recipe `parts`, UI `children`), so two agents adding different items to one list
both land; arrays of strings where the kind says they are sets (tags) merge as sets; other arrays merge position by
position when all three have the same length, and are atomic otherwise. A value both sides changed differently is a
conflict: listed with its JSON path and the three values, and the merged result keeps `theirs` there (never silently
overwriting someone's newer change); `prefer: "yours"` keeps yours instead. Bounded: 256 KiB per side, depth 64,
256 conflicts listed.

## The director pattern (M4 part)

One lead agent turns a design-doc section into work, assigns it by role, tracks it, and puts the result in front of
the owner. The workflow:

1. **Plan**: `task_plan {file, section}` reads a Markdown section (the repo's docs, `.md` only, at most 1 MiB) and
   turns its unchecked checklist items into tasks under one plan task: nested items become subtasks, an item's
   indented lines its detail, `#role` words its role (`#builder`, `#artist`, `#playtester`, `#reviewer`), and
   `(after: <words>)` a dependency on the earlier item that starts with those words. Or `items: [{key, title, detail,
   role, after: [keys]}]` straight from the lead's own reading. `dry_run` shows the plan without posting it.
   Posting is all or nothing.
2. **Assign roles**: `assign: "by_role"` gives each task with a role to the connected agents whose `agent_hello` role
   matches, round-robin in name order (deterministic); tasks whose role nobody has stay open for whoever comes.
   Every task is tagged `role:<role>` and `plan:<id>`, so a worker asks `task_list {ready_for_me: true, tag:
   "role:builder"}`.
3. **Track**: `task_progress {plan}` is the lead's dashboard: counts by status and by role, percent done, blocked
   tasks with their blockers, open questions, and the plan's review state (submissions pending, approved, rejected,
   open critiques), plus the next checkpoint ("4 submissions wait for the owner").
4. **Checkpoint**: workers submit to the review queue with their task; the lead (or the owner, at the window) decides
   batches; rejections come back as rework tasks in the same plan; when every task is done and every submission is
   decided, `task_progress` says `complete`.

`scripts/review_check.py` runs it end to end with a scripted lead and three workers.

## Not in this lane

- **Binary assets through the protocol** (uploading images and audio over MCP with claims, versions and the journal)
  is left to the project-data lane: it owns files as versioned project data, and a second upload path here would be
  the clobbering the swarm track exists to stop. Captures are referenced by path because the engine wrote them.
- **Plain-text merges** (line-based diff3) of files without a schema: not built; the merge is offered only where
  the engine knows the shape.
- **Role-scoped tool lists** (TOOLS.md): the review and critique tools add about 10 KB to `tools/list`.
