# Cloud swarms: foundations

Status: **design notes only** (the end of M5, 2026-11-12). Nothing here is built. M6 builds it, tests it with real
swarms from several vendors, and puts it through an adversarial review before any marketing (ROADMAP, M6).

The owner's direction (2026-10-22): agents stay local until M6; the foundation notes are laid at the end of M5;
encryption always comes from the operating system, never from our own code (AGENTS.md rule 12). And from M5
(2026-11-07): **every agent is untrusted, ours and other companies' alike.**

## What changes when an agent isn't on this machine

Today the engine listens on loopback only, and that one fact carries most of its security:

- **Whoever can connect is a program the operator ran.** There are no credentials. A session is whatever opened a
  connection.
- **Identity is a name.** `agent_hello {name}` makes an identity; a resume token (a random string the engine hands
  out) carries it to a later session.
- **Authority is a name too.** `--owner NAME`, `--lead NAME` and `--reviewer NAME` grant a role to *the first
  identity that says that name*. (Until 2026-11-12, with no `--lead` at all, the first agent to say "lead" also
  became the lead. The owner removed that: a role an agent gives itself is no role.)

M5's security reviews kept running into the same wall, and each time the honest answer was "until M6":

| Found in M5 | Why it can't be closed with names |
|---|---|
| An agent that says hello under a granted lead's name before the real one connects *is* that lead ([SECURITY.md](SECURITY.md), "Names until M6") | a name proves nothing |
| A frozen agent can open a new connection and be someone else | an identity costs nothing to make |
| Any client with two identities can pause the swarm, again and again | same |
| A self-declared lead had real powers, and got out of the monitor's freezes with them (removed 2026-11-12: [AUTHORITY.md](AUTHORITY.md)) | the role was claimed, not given |
| Tool answers hold paths on the operator's disk (a build's `path`, capture files) | harmless on one machine, a leak to a remote one |

Remote agents turn each of these from a local nuisance into a way in. So the foundation is not "open the port":
it is **credentials the operator issues, and everything else hanging off them**.

## Goals and non-goals

**Goals**
- Agents on other machines, from any vendor, work on a world with the same tools as local ones.
- The operator decides who connects, as whom, with which tools, and can take it back at once.
- A stolen or misbehaving credential is contained: it reaches only what it was scoped to, at a bounded rate.
- Loopback with no credentials stays the default and keeps working exactly as it does today.
- Zero third-party crates, and no cryptography of our own (rule 12).

**Not goals**
- Accounts, sign-up, a hosted service or a relay. Specula is an engine; operators and their organisations secure
  their own environments.
- NAT traversal. A tunnel or a reachable host is the operator's to provide.
- A sandbox for native game modules. That is its own decision ([MODULES.md](MODULES.md), "M6: a sandbox for
  untrusted modules"); until it exists, remote agents load no native code at all (below).
- Streaming the window to a remote human. Remote humans use captures and sight like agents do.

## Who we defend against

| Who | What they have | What they want |
|---|---|---|
| A remote agent with a valid credential that misbehaves (buggy, prompt-injected, or hostile by design) | everything its credential allows | to wreck or steal the world, run code on the host, reach the human's input, burn the operator's money |
| Someone holding a stolen credential | the same, until revoked | the same |
| Someone on the network path | packets | to read or change the traffic |
| A scanner that finds the port | a TCP connection | anything unauthenticated |
| A web page in the operator's browser | requests from the operator's machine | to reach the engine through the browser (DNS rebinding, cross-site requests) |
| A compromised agent host | its agent's credential | to move from that agent to the engine's host |

Out of scope: the operator's own machine or account being compromised. Whoever has that already has everything
the engine could protect.

## 1. Credentials: per-agent tokens

**The operator issues a token per agent.** A token is 256 random bits from the operating system's generator
(`BCryptGenRandom`, `getrandom`, `SecRandomCopyBytes`), shown once, in the form `spc_<id>_<secret>`: the `id` is
public (logs, the security monitor, the journal); the secret never appears again.

```
specula token new --name ann --role builder --tools world,sight,models --expires 8h
specula token list
specula token revoke <id>
```

**What the engine keeps** is a tokens file the operator names (`--tokens FILE`), holding each token's id, the
SHA-256 of its secret (the OS's digest, as `specula_sys::digest` already uses for review evidence) and its grants.
It lives **outside the project folder**: agents read project files. The engine compares hashes in constant time
and rereads the file when it changes (so a revoke takes effect without a restart).

**What a token carries**

| Field | Meaning |
|---|---|
| `name` | the agent's identity: what the journal, claims and undo say. `agent_hello` may not say another |
| `role` | builder, reviewer, lead or owner: the authority grant, bound to this token (not to whoever says a name first) |
| `tools` | the ceiling of its tool list (families from `toolscope`); the lead's `tools_scope` can only narrow it |
| `capabilities` | the dangerous things, each off unless named: `native` (module code), `window` (the human's keyboard, screens and input), `source` (module source files), `disk` (prune) |
| `world` | optional: the subtrees and regions it may write (enforced as standing claims) |
| `rate` | its rate class (section 3) |
| `expires` | required for remote tokens; at most 30 days |
| `sessions` | how many sessions it may hold at once (default 2) |

**How it is presented:** `Authorization: Bearer spc_...` on every HTTP request, checked on the connection thread
before anything is parsed further. A missing, unknown, expired or revoked token gets `401` and a security event
(section 4), never a session.

**What this replaces**
- The identity key becomes the token's id. Claims, the journal, undo, freezes and resume all hang off it: the
  roadmap's "claims tied to tokens". Resume tokens stay for local, token-less agents only.
- Grants by name (`--lead NAME`) stay for local use. In token mode a role comes from a token or not at all:
  no first-come binding of a granted name, as there is already no first-come lead.
- A freeze holds the token. A frozen agent that reconnects is the same frozen identity, because it has to present
  the same token.

**Two modes**
- **Local trust** (today, the default): loopback, no tokens needed.
- **Token mode** (`--require-tokens`): every connection needs a token, loopback included. This is the right mode
  whenever agents the operator doesn't control run on the same machine, cloud or not, and it is required for any
  bind beyond loopback.

## 2. Transport: when the port may open

**The engine refuses to listen beyond loopback unless it has both tokens and encryption.** `--bind` with a
non-loopback address fails at start, naming what is missing. There is no flag to skip this.

Encryption comes from the operating system or from a tunnel, never from us:

| Host | Encryption |
|---|---|
| Windows | SChannel (SSPI), declared through raw-dylib like every OS import, with the certificate taken from the Windows certificate store by thumbprint |
| macOS | Network.framework, with an identity from the keychain |
| Linux | **no OS TLS: a tunnel is required** (SSH port forwarding, WireGuard or Tailscale). The engine binds to loopback or to the tunnel's interface only |

**A tunnel is the recommended first shape on every OS.** The engine stays on loopback, no new code path faces the
network, and the tunnel does the encryption and the peer authentication. Tokens are still required: a tunnel says
which machine is calling, not which agent, and not what it may do.

Rules for the TLS path when it is built:
- The OS's defaults decide protocol versions and ciphers (TLS 1.2 or newer). No option weakens them.
- The operator supplies the certificate. The engine never makes one.
- Client certificates (mutual TLS) are a later option, on top of tokens, never instead of them.
- New DLL owners (`secur32`/`sspicli`, `crypt32`) follow rule 2: one crate each, listed in AGENTS.md.

**The HTTP server before it faces a network** (it was written for loopback):
- **Host allow-list.** Requests whose `Host` isn't one the operator named are refused (DNS rebinding). The
  existing `Origin` check stays; there is no CORS.
- **Timeouts**: for headers, for a body, and for an idle connection, so slow clients can't hold threads.
- **Limits per token and per address**: connections, sessions, in-flight calls.
- **No answer before authentication** beyond `401`: no tool list, no engine version, no session id.

## 3. What a token may do, and how much

### Authority comes from the token

`authority::trust` (M5) already separates "a lead the operator granted" from "an agent that said lead". In token
mode that becomes the only kind: authority is what the token says.

What a remote token gets **by default**, compared with a local grant:

| | Local lead (`--lead`) | Remote lead token | Remote builder token |
|---|---|---|---|
| World, sight, models, playtests | yes | yes, within `tools` and `world` | yes, within `tools` and `world` |
| Coordination (tool scopes, the frame floor, forcing over claims) | yes | yes | no |
| Reviews: decide, open the review screen | yes | decide yes; the screen needs `window` | no |
| The human's keyboard, galleries on the window, input holds | alert, then allowed | **no** unless `window` | no |
| Building and loading native module code | yes | **no** unless `native` | proposes only |
| Module source files (`.rs`, `Cargo.toml`) through project tools | yes | **no** unless `source` | no |
| `security_clear`, `security_incidents` | yes | yes | no |
| Owner authority | by `--owner` | **never remotely** by default; an owner token needs `--allow-remote-owner` | no |

The reasoning for the three "no unless" rows: each is a way from the world to the host or to the human. A remote
agent that can load native code owns the machine; one that can point the keyboard reads what the human types.
They stay closed until the operator names them on a token, and the monitor still alerts on every use.

### Rates and budgets

Every limit is per token, refuses with data (`rate_limited`, `retry_after_s`), and is visible to the lead:

| Limit | Enforced where |
|---|---|
| Calls per second (a token bucket with a small burst) | the connection thread, before routing |
| Calls in flight | the connection thread |
| Bytes in and out per minute | the HTTP layer |
| Captures and other GPU work per minute | the capture queue (a GPU-time lease per token) |
| Worker jobs waiting | `Engine::offload`'s byte and slot caps, split per identity |
| Disk written (project files, saves, uploads) | the disk book, a quota per token |
| Entities, claims, journal entries per hour | the write policy |
| Sessions | the session table |

**Fairness.** `schedule.rs` serves agents' calls between presents. It needs per-identity round-robin, so one
token's flood can't starve the others or the window. The 64-agent drill gets a hostile-agent variant.

**Cost.** Token use per agent is reported by each agent's harness, not known to the engine. The dashboard shows
what the engine can measure (calls, commits, refusals, GPU and worker time per token) beside what harnesses report.

## 4. Seeing it and stopping it

The input monitor ([SECURITY.md](SECURITY.md)) grows a channel for the door itself:

- **Authentication events**: a bad, expired or revoked token; a `Host` or `Origin` refused; a rate limit hit hard.
  Each with the token's id (when there is one) and the source address.
- **Backoff per address** after repeated failures, in a bounded table, so guessing is slow and can't fill memory.
- **Freezes hold tokens.** The swarm-pause rule (a second offender pauses everyone) becomes a policy the operator
  sets for remote swarms: with strangers connected, one hostile pair shouldn't be able to stop the paying work.
- **Issuing and revoking tokens is journaled** (`kind: security`), ids only.
- **The kill switch.** The owner or a lead with `window`-level trust closes the door at runtime: remote sessions
  end, their claims and leases release, and the engine goes back to loopback.

## 5. What a remote agent can read

On one machine every agent could already read the disk, so answers were never scrubbed. Remotely they must be:

- **Paths.** No answer, error, trace or capture packet may hold an absolute path of the operator's machine (they
  give away the user name and the layout). `private_paths` checks committed files today; a second guard checks
  tool answers. Paths go out relative to the project, or as opaque handles the engine resolves.
- **Captures and files** are fetched through the protocol (as `project_upload` already sends them in), never by
  path.
- **Read scope.** A token's `world` scope can limit reads as well as writes (another studio's level, an unreleased
  character).
- **Traces and incidents** already answer by authority; token ids appear, secrets never.
- **The human's typing** is already the owner's alone ([SECURITY.md](SECURITY.md), R3).
- **Secrets stay out of the project folder.** The tokens file, certificates and anything else private live where
  no project tool can reach.

## 6. What doesn't change

- **Determinism.** Calls are journaled in the order they arrive, as now. A remote agent's latency changes when its
  call lands, never what a recorded run replays to.
- **The write policy, claims and versions.** Latency makes conflicts likelier; `expect_version`, claims and the
  change feed are already how agents handle them.
- **Local use.** No token, no TLS, nothing new to set up.

## Shapes we expect

1. **Several vendors' agents on one machine.** Loopback, `--require-tokens`. No network at all. This is the first
   test in M6, and the cheapest.
2. **A small team over a tunnel.** The engine on loopback on one machine; teammates' agents reach it through
   Tailscale, WireGuard or SSH, each with a token.
3. **A headless engine on a cloud host.** Agents from anywhere, TLS from the host's OS (or a tunnel on Linux), the
   human directing through sight and the review tools from their own agent.
4. **The engine on the operator's desktop, agents in the cloud.** Works through a tunnel. Opening a home router's
   port for it is discouraged, and the docs will say so.

## Build order for M6

Each step lands with drill rows and an adversarial probe, like every security change in M5.

1. **Tokens**: the file format, `specula token` commands, hashing through the OS, bearer checks, `--require-tokens`.
2. **Authority from tokens**: identity keys are token ids; no fallback lead in token mode; the capability table.
3. **Rates and fairness**: per-token limits, per-identity scheduling, the hostile-agent drill.
4. **The HTTP server hardened**: `Host` allow-list, timeouts, per-token and per-address caps.
5. **The read side**: the answer-path guard, files by handle, read scopes.
6. **The monitor's door channel**, backoff, the kill switch.
7. **The bind rule**, then shape 1 and shape 2 tested with real swarms from several vendors.
8. **SChannel on Windows**, then Network.framework; Linux stays on tunnels. Shape 3.
9. **The adversarial review** of the whole engine, cloud access included, before any marketing.

Steps 1 to 7 need no encryption code at all, and they are where the protection is. TLS (step 8) widens where the
engine can run; it doesn't make an unauthenticated engine safe.

## Decisions for the owner

The questions this design leaves open are in the one list, [HANDOFF.md](../../HANDOFF.md#open-owner-decisions).
