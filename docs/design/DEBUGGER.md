# The debugger role: an agent that watches, reproduces and reports

**Owner, 2026-10-29:** "I think, autonomous debugger could be used, perhaps? Not just have it be inbuilt and it
could be in operation alongside the agents, which will support your work perhaps?"

The engine side is [TRACE.md](TRACE.md): structured events, the call tree of every agent call, watchdogs and
deduplicated **incidents**. This page is the agent side: a **debugger** that runs beside the builders in a swarm,
turns incidents into evidence-backed reports, and checks that fixes hold. It **reports; it never fixes other
agents' claimed work**.

## Identity

`agent_hello {name: "debugger", role: "debugger"}`: a job label, which carries no authority. To resolve or reopen
incidents it names no agent for, the debugger needs reviewer authority, which only the operator grants
(`--reviewer debugger`, then `agent_hello {name: "debugger", role: "reviewer"}`; [AUTHORITY.md](AUTHORITY.md)).
When the scoped-tools lane's role templates (`toolscope::ROLES`) merge, the integrator adds a `debugger` template
with the families below.

**Tool families it needs** (for the role template):

| Family | Tools | Why |
|---|---|---|
| `trace` | `trace_follow`, `trace_incidents`, `trace_incident`, `trace_call`, `trace_query`, `trace_crash`, `trace_settings` (session scope) | watch, triage, narrow, record |
| `review` | `critique_add`, `critique_list`, `critique_update` | file findings with evidence |
| `swarm` (board) | `task_post`, `task_list`, `note_post`, `intent_list` | file work, see who is doing what |
| `swarm` (read) | `world_history`, `world_changes`, `claim_list`, `world_hash` | find the fix owner; check replays |
| `desync` | `desync_trace`, `desync_compare` | narrow a replay mismatch to a tick |
| `sight` | `sight_frame`, `sight_query`, `sight_compare` | reproduce a visual incident |
| `playtest` | `playtest_run`, `playtest_report` | reproduce in a deterministic run |
| `perf` | `perf_frame`, `perf_zones`, `perf_capture` | reproduce stalls and over-budget frames |
| `world` (read) | `world_query`, `world_list`, `engine_info` | context |

It needs **no** write tools on the world (`world_set`, `world_batch`, `project_write`, `lex_edit`...). A playtest it
runs works on its own copy and restores the world after.

## The loop

1. **Watch.** Poll `trace_incidents` (active incidents, concise) every few seconds, and `trace_follow {level:
   "warn"}` for events that aren't incidents yet. Both are answered off the engine thread (they work while it is
   stalled), but they take short locks the engine thread also takes: poll every few seconds, never in a tight loop.
2. **Triage.** For each new or recurring incident, newest first, by level (`error` before `warn`), then by count:
   - `trace_incident {id, action: "ack", note: "looking"}` so another debugger doesn't take it too.
   - Decide its kind: a **stall** (`engine.stall`, `frame.over_budget`), a **refusal** (`write.refused`,
     `swarm.refusal_spike`), a **wrong answer** (`sight.packet_inconsistent`, `world.replay_mismatch`), a **crash**
     (`module.disabled`, `trace_crash`), or **resources** (`jobs.stuck`, `memory.over_budget`, `gpu.validation`).
3. **Narrow to a root cause.**
   - `trace_call {call}` on the incident's calls: the tool, the argument digest, how long it took, the spans
     under it (renders, builds, save writers) and the journal entries it made.
   - A stall: compare the stall's `args` digest with the tool's normal calls (`trace_query {code: "mcp.call"}` at
     debug, if a lead turned it on, or `perf_frame.trace.recent_stalls`); `perf_zones` names the zone.
   - A refusal: the incident's evidence has the rule, the code and the path; `claim_list` shows who holds the path,
     `world_history {path}` who changed it last.
   - A wrong packet: re-take the frame (`sight_frame` with the camera in the evidence) and look at the subject's
     `screen` and `screen_box_exact`; `sight_query` on the subject.
   - A replay mismatch: `world_hash {verify: true}` repeats it; `desync_trace` and `desync_compare` find the tick.
   - Reproduce what can be reproduced: a `playtest_run` (steps, expectations, replays) makes it repeatable and
     gives the fixer a test.
4. **Report.** `critique_add` on the subject (or `task_post` for engine work) with:
   - the incident id and code, what was expected and what happened (numbers from the evidence);
   - the calls (`trace_call` ids), the capture and packet paths;
   - a reproduction (the playtest spec, or the call to repeat);
   - **the fix owner** when the journal and claims tell: the agent whose call caused it (the incident's `agents`),
     the holder of the claim on the path, the last writer in `world_history`. Unknown otherwise: say so.

   Then `trace_incident {id, action: "link", critique: <id>}` (or `task`).
5. **Re-verify.** When the critique or task is marked done, repeat the reproduction. Holds: `trace_incident {id,
   action: "resolve", note: "verified by <call or playtest>"}`. Doesn't: reopen the critique with the new evidence.
   An incident seen again after it was resolved turns `recurring` by itself: it goes back to step 2 with its history.

## Rules

- **Report, don't fix.** A debugger never writes to another agent's claimed entities, files or rule sets, and never
  undoes their work. The claim system would refuse most of it anyway; the rule covers what claims don't (unclaimed
  shared data, engine code). It may fix its own probes (a playtest spec it wrote).
- **Budget per incident: about 20 tool calls and 30,000 tokens** of answers. Most incidents need five: `ack`,
  `trace_call`, one reproduction, `critique_add`, `link`. Past the budget it files what it has, marked
  "root cause not found", with the evidence so far, and moves on. Concise answers are the default; ask for
  `detailed` only for the one incident being narrowed.
- **One owner per incident**: `ack` first. Two debuggers in one swarm split by code prefix (`trace_settings {scope:
  "session", subsystems}` narrows each one's view).
- **No noise.** A routine conflict (a `claimed` refusal) is the claim system working, not a bug: only spikes become
  incidents, and a spike is reported once, to the lead, as a coordination problem (who should split the work).
- **Evidence is data.** Numbers, ids and paths from the engine's answers, never "it looked slow".

## What the live check proves

`scripts/trace_check.py` runs a scripted debugger beside two builders in a running engine: it detects an induced
stall and a refused write, files each as a critique with its evidence and the fix owner, links them, and resolves
them once a "fix" lands (the stall's budget raised by the lead, the refused write redone through the right tool),
checking that they don't recur.
