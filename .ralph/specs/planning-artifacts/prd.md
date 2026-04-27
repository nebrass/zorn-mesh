---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary', 'step-01b-continue', 'step-03-success', 'step-04-journeys', 'step-05-domain', 'step-06-innovation', 'step-07-project-type', 'step-08-scoping', 'step-09-functional', 'step-10-nonfunctional', 'step-11-polish', 'step-12-complete', 'step-e-01-discovery', 'step-e-02-review', 'step-e-03-edit']
workflowComplete: true
completedAt: '2026-04-27'
lastEdited: '2026-04-27'
workflow: 'edit'
releaseMode: phased
inputDocuments:
  - "_bmad-output/project-context.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh-distillate.md"
  - "_bmad-output/planning-artifacts/ux-design-specification.md"
  - "_bmad-output/planning-artifacts/architecture.md"
documentCounts:
  briefs: 1
  distillates: 1
  research: 0
  brainstorming: 0
  projectDocs: 0
  projectContext: 1
projectStatus: 'greenfield'
workflowType: 'prd'
project_name: 'zorn-mesh'
user_name: 'Nebrass'
date: '2026-04-26'
classification:
  projectType: 'developer_tool'
  projectTypeAddenda: ['cli_tool', 'system_daemon_informal', 'local_web_control_plane']
  domain: 'developer_infra'  # informal — supersedes BMad CSV's 'general'; complexity override required
  domainCsvFallback: 'general'  # for any tooling that reads CSV taxonomy
  complexity: 'high'
  projectContext: 'greenfield'
  primaryJob: 'observability_of_broken_multi_agent_system'  # "know exactly what happened, and when"
  primaryPersona: 'individual_developer'  # buyer = user at v0.1
  futurePersona: 'platform_engineer'  # buyer ≠ user, flagged so v0.1 decisions don't quietly foreclose
  launchGate: 'mcp_stdio_bridge_and_local_web_control_plane'  # forcing functions — host interop plus observable local UI at v0.1
  mandatoryAddenda:
    - 'operations_and_lifecycle_section'  # daemon state machine, socket ownership, SQLite contract, upgrade protocol, crash recovery, zornmesh doctor spec
    - 'compliance_and_regulatory_section'  # EU AI Act, NIST AI RMF, NIST SP 800-218A, GDPR, SOC 2, CISA SBOM
    - 'stakeholder_map_section'  # enterprise security reviewers, agent-runtime vendors, protocol stewards, OSS contributors, enterprise buyers, regulatory watch
  classificationProvenance: 'party-mode review by Mary (BA), John (PM), Winston (Architect) — 8 enrichments accepted by user'
editHistory:
  - date: '2026-04-27'
    changes: 'Reconciled PRD with completed UX spec: local web companion UI promoted to v0.1, CLI-only/no-GUI contradictions removed, and UI FR/NFR/security/accessibility coverage added.'
  - date: '2026-04-27'
    changes: 'Validation quick fix: removed implementation/tooling leakage examples from NFRs while preserving observable acceptance criteria.'
  - date: '2026-04-27'
    changes: 'Validation quick fix: added mandatory Operations & Lifecycle and Stakeholder Map sections, promoted Compliance & Regulatory to a dedicated section, and removed the v0.5 roadmap placeholder.'
  - date: '2026-04-27'
    changes: 'Validation warning cleanup: added UI scope supersession note and tightened measurable FR/NFR acceptance language including FR44.'
---

# Product Requirements Document — zorn-mesh

**Author:** Nebrass
**Date:** 2026-04-26
**Status:** Complete (12 of 12 steps) — ready for downstream architecture / epic breakdown

## Executive Summary

Zorn Mesh is a single-binary local message broker, protocol adapter, and local web control plane that lets AI coding agents — Claude Desktop, Cursor, Copilot, Gemini, plus any custom Python/TypeScript/Rust agent — discover each other, exchange typed messages, replay history, and be inspected on a single developer machine, without a network broker.

**Three agents on your laptop, talking. Trace anything. No broker.**

The 2026 protocol landscape has settled: MCP 2025-11-25 owns tool-calling, A2A 1.0.0 owns agent-peer RPC and AgentCard shape, AGNTCY/SLIM is emerging for federated overlays. None solve local coordination. A developer running 3–5 agents in parallel today has zero infrastructure for them to coordinate — they cope with random ports, `/tmp` JSON files, and ad-hoc orchestration. When a multi-agent pipeline breaks at 2 a.m., they have nothing to look at, and the deepest pain follows mechanically: when no one can answer "what did the agents actually do," no one builds the next agent.

Zorn Mesh fills that gap. The bet: **MCP is the adoption wedge, NATS is the model, SQLite is the store, Rust is the runtime, and a local browser is the calm control room.** No new ideas — just the right ones, packaged for the developer machine. A privileged-by-uid broker daemon auto-spawns on first SDK connect (the `sccache` pattern), owning a Unix-domain socket and a single SQLite file at `~/.local/state/zornmesh/mesh.db`. Agents call `Mesh.connect()`, register, advertise capabilities, and immediately publish to topics, request from peers, subscribe with NATS-style hierarchical wildcards, and stream chunks over the architecture-defined local mesh transport. Existing clients (Claude Desktop, Cursor) join the bus via `zornmesh stdio --as-agent <id>` without modification, while developers can open `zornmesh ui` to see connected agents, inspect ordered traces, and safely send direct or broadcast messages.

The strategic positioning is **witness, not just broker.** The audit log is testimony; `correlation_id` is a thread of memory; local-first is the authority of observation returned to the developer's own machine. The product refutes the assumption — baked into every multi-agent framework today — that internal agent behavior need not be observable. The antagonist is **the unobservable**; the archetypal need this product satisfies is the need to witness.

### Scope Supersession Note

This edited PRD supersedes earlier product-brief and distillate statements that deferred a "web dashboard" or treated UI as non-core. v0.1 includes a constrained **local web companion UI** for observing the mesh, inspecting trace chronology, sending safely, and confirming outcomes. Hosted/cloud dashboards, LAN/public consoles, accounts/teams, workflow editors, full chat workspaces, and remote collaboration remain out of scope.

### What Makes This Special

- **Three-property compound, not three features.** Local-first by architecture (no broker to install) + MCP-superset wire (no opt-in friction; existing MCP clients work as-is via the stdio bridge) + single 22 MB Rust binary (no operations burden). Any two of those and you're a developer tool. All three at once and you're the SQLite of agent buses.
- **The killer feature is forensic.** `zornmesh trace <correlation_id>` and the local Focus Trace Reader reconstruct a multi-agent conversation from the same underlying trace model — Jaeger-quality forensics with zero infrastructure setup. The first time a developer clicks or pastes a UUID and watches the conversation rebuild, the agents stop being black boxes.
- **Symmetric capability model**, unlike MCP's hub-and-spoke. Both ends advertise both consumer and provider sets at handshake. Native peer-to-peer feel rather than tunneled.
- **Inspectable by design.** SQLite store, Unix-domain socket, JSON wire, OpenTelemetry tracing, CLI, and local web UI all expose the same truth. `sqlite3`, `socat`, `zornmesh tail`, `zornmesh trace`, and the Live Mesh Control Room are first-class debug surfaces. The 2 a.m. on-call story can be a copied CLI recovery command or `sqlite3 mesh.db 'SELECT * FROM messages WHERE stream=? ORDER BY offset DESC LIMIT 50'`; the UI explains the chronology before the operator reaches for either.
- **Honest reliability claims.** At-least-once delivery with mandatory idempotency keys, full-jitter exponential backoff, dead-letter queue, lease-based pull delivery. Exactly-once is explicitly rejected — Jepsen 2026 demonstrated it is unattainable even for NATS JetStream, and lying about it erodes developer trust.
- **The conceptual moat is `mesh-trace/1.0`** — a published open standard for multi-agent conversation replay (correlation IDs, causality chains, message boundaries, agent identities), shipped *before* public MVP. When "how do I audit my agent mesh?" becomes the universal question (~6 months out), the answer should already have a name, and `zornmesh trace` should be the reference implementation.

## Project Classification

| Field | Value |
|---|---|
| Project type | `developer_tool` (with `cli_tool`, local web control-plane, and system-daemon characteristics) |
| Domain | `developer_infra` (informal — supersedes BMad CSV's `general`; complexity override required) |
| Complexity | `high` |
| Project context | `greenfield` |
| Primary job | Observability of a broken multi-agent system at 2 a.m. — "know exactly what happened, and when" |
| Primary persona (v0.1) | Individual developer (buyer = user) |
| Future persona (v0.5+) | Platform engineer at mid-size org adopting agent fleets (buyer ≠ user) |
| Launch gate | MCP-stdio bridge interoperating with at least one major host (Claude Desktop or Cursor) and local web UI completing observe → inspect trace → send safely at v0.1 |
| Mandatory PRD addenda | Operations & Lifecycle section · Compliance & Regulatory section · Stakeholder Map section |

## Success Criteria

### User Success

The "aha!" moment is forensic, not merely functional: a developer clicks a trace in the local UI or pastes a UUID into `zornmesh trace <correlation_id>` and watches a multi-agent conversation rebuild in order. Five concrete user-success states anchor v0.1:

- **First-coordinated-message moment (v0.1):** a developer who has never seen the tool runs `cargo install zornmesh` (or `npm i @zornmesh/sdk`), opens `examples/three_agents.rs`, and sees three agents exchange envelopes through the bus end-to-end in **under 10 minutes wall-clock** (target: median **< 600 s** measured via opt-in install-to-first-message telemetry, off by default).
- **Live mesh visibility moment (v0.1):** a developer runs `zornmesh ui`, the browser opens to a loopback-only local control plane, and connected agents appear live without manual setup. Success: at least 3 connected agents render with status, capabilities summary, transport/source, and last-message recency within **2 seconds** of daemon readiness.
- **2 a.m. forensic moment (v0.1):** when a multi-agent pipeline breaks, the developer has a complete answer — the Focus Trace Reader and `zornmesh trace <id>` each produce a full ordered timeline (every envelope, every hop, with timing) for the failing correlation ID with **zero additional setup** beyond what they already had running. Success is measured by (a) neither surface returning empty for a correlation_id observed in the audit log within retention, and (b) the timeline reconstructs across at least 3 hops without gap.
- **Existing MCP host joins without modification (v0.1):** a Claude Desktop or Cursor user runs `zornmesh stdio --as-agent <id>` and the existing host treats Zorn Mesh as a regular MCP server with **zero config changes** to the host. Success: at least one major host completes a tool call through the bridge against an unmodified released build.
- **Safe intervention moment (v0.1):** a developer sends a direct message to one agent or broadcasts to selected agents from the local UI after previewing recipients and payload summary. Success: the UI shows per-recipient delivery state, partial failures, and equivalent CLI command within **5 seconds** for a 3-agent example.

### Business Success

This is a developer-tool / open-source play; business metrics are adoption, ecosystem traction, and standards influence — explicitly **not** revenue at v0.1.

- **v0.1 (launch gate):** MCP-stdio bridge interoperates with at least **one major host** (Claude Desktop or Cursor) on a released build. **Zero CVEs at launch.** Linux + macOS only.
- **v0.5:** **1,000 weekly active developers**, **≥1 major agent runtime** ships a Zorn Mesh adapter in default config, **≥1 production A2A bridge deployment** outside the core team.
- **v1.0:** the MCP-superset wire is recognized as a deployable profile in **≥1 independent agent framework**; multi-host federation demonstrated in production by an external adopter.
- **Conceptual moat:** `mesh-trace/1.0` published as an open standard for multi-agent conversation replay **before** public MVP; `zornmesh trace` is the reference implementation cited in adopter docs.

**Anti-metrics (do NOT chase):** peak msg/s throughput numbers, GitHub stars, npm download counts, vanity downloads.

### Technical Success

These are correctness gates and operational budgets — **all** must pass for v0.1 to ship.

**MVP correctness gates (cannot ship without all):**

1. Every `MessageType` round-trips through codec property tests (`proptest`) without loss.
2. Lease reaper survives **10,000 simulated agent crashes** in `turmoil` without losing or duplicating an envelope outside the at-least-once contract.
3. SQLite schema migrates cleanly from empty DB under `cargo test --features migration-stress`; every numbered migration applies forward.
4. `zornmesh trace <id>` produces a complete timeline for the 3-agent example.
5. Browser E2E fixture completes observe → inspect trace → direct send → broadcast with recipient preview and per-recipient outcomes against the 3-agent example.
6. Local UI security fixture proves loopback-only bind, session token requirement, CSRF/origin protection, and zero external asset fetches.
7. Accessibility baseline passes WCAG AA contrast, keyboard-only navigation, reduced-motion behavior, and no color-only status for core UI flows.
8. Branch coverage **≥ 90%** on routing-critical modules (router, idempotency gate, control-frame discriminator); line coverage **≥ 70%** on every package; coverage cannot regress per PR.
9. `buf breaking` passes against the previous tag — wire stability enforced.

**Operational budgets (machine-enforced where possible):**

- Daemon **cold-start ≤ 200 ms** to "accepting connections" (auto-spawn invariant).
- SDK connect retry budget **≤ 1 s** with exponential backoff.
- Writer transaction budget **≤ 50 ms** measured from inbox dequeue (not `BEGIN`).
- Graceful shutdown completes **within 10 s** of SIGTERM (configurable, max 60 s).
- Daemon RSS target **5–15 MB** at idle (vs Go-runtime daemons' 30–80 MB).
- Single static Rust binary **≤ 22 MB**.
- Memory ceiling: **~96 MiB SQLite cache + ~64 MiB mapped DB**.

**Honest reliability claims:** at-least-once delivery, mandatory idempotency keys, full-jitter exponential backoff, dead-letter queue, lease-based pull delivery. **Exactly-once is explicitly rejected** — Jepsen 2026 demonstrated it is unattainable even for NATS JetStream; lying about it would erode trust.

**Performance is tracked, not gated at the latency layer** (correctness first): p50 / p95 / p99 envelope latency under sustained **1,000 env/sec baseline** load + DB transaction p95/p99 are tracked weekly on dedicated infrastructure; **week-over-week >20% regression auto-files an issue**. Sustained-throughput targets (≥ 5,000 env/sec end-to-end, see NFR-P2) are a separate, gated CI benchmark; the two tests measure different properties and use different workloads.

### Measurable Outcomes

| # | Outcome | Target | Measurement |
|---|---|---|---|
| 1 | Time-to-first-coordinated-message (North Star) | median **< 600 s** | Opt-in telemetry from install → first envelope; or self-report from onboarding survey |
| 2 | Cold-start latency | **≤ 200 ms** | CI integration test (auto-spawn lifecycle) |
| 3 | Writer transaction p95 | **≤ 50 ms** | Weekly load harness, dashboard regression alerts |
| 4 | MVP correctness gates | **9/9 pass** | Required CI gate; release blocker |
| 5 | Routing-module branch coverage | **≥ 90%** | `cargo nextest` + coverage gate per PR |
| 6 | Major MCP host interop (v0.1 launch gate) | **≥ 1 host** | End-to-end interop test against released host build |
| 7 | Local UI trace comprehension (v0.1) | **100% core fixture pass** | Browser E2E: open UI → click trace → ordered complete timeline with detail pane |
| 8 | Safe direct/broadcast send (v0.1) | **100% core fixture pass** | Browser E2E: preview recipients → confirm → per-recipient outcomes |
| 9 | UI accessibility baseline (v0.1) | **WCAG AA for core flows** | Automated checks + keyboard-only walkthrough fixture |
| 10 | Weekly active developers (v0.5) | **≥ 1,000** | Anonymous opt-in usage ping (daemon-startup ping with random ID, off by default) |
| 11 | Major-runtime adapter shipped (v0.5) | **≥ 1** | External adopter announcement + integration test in their repo |
| 12 | Independent framework recognizing wire (v1.0) | **≥ 1** | Public reference in adopter docs |
| 13 | CVEs at launch | **0** | `cargo audit` + JS dependency audit clean; SBOM published per release |

## Product Scope

### MVP — v0.1 (Minimum Viable Product, ~10 weeks)

**In scope:**

- Single-binary `zornmesh` daemon, statically compiled (Linux + macOS).
- Agent registration with capabilities; presence + heartbeats.
- Request/reply with `correlation_id` and cancellation.
- Fire-and-forget events.
- Topic pub/sub with `*` (single-segment) and `>` (trailing) wildcards.
- Streaming chunks with 256 KiB byte-budget backpressure window.
- Append-only SQLite message log with offset-based replay.
- At-least-once delivery, mandatory idempotency keys, lease-based pull delivery, DLQ.
- OS-level trust: UID match (`SO_PEERCRED`) + socket ACL (`chmod 0600` socket in `chmod 0700` dir); abstract Unix sockets explicitly rejected.
- Architecture-defined local mesh transport with MCP 2025-11-25 compatibility at the stdio bridge boundary.
- MCP-stdio bridge: `zornmesh stdio --as-agent <id>` (the **launch-gate forcing function**).
- Local web companion UI launched by `zornmesh ui`: live agent roster, message/trace timeline, Focus Trace Reader, daemon/trust status, safe direct send, safe broadcast, and CLI handoff copy blocks.
- `zornmesh` CLI: `tail`, `trace`, `agents`, `inspect`, `doctor`, `replay`, `stdio`, `daemon`, `ui`.
- Rust SDK and TypeScript SDK at parity (Bun-first first-party TypeScript toolchain; npm package remains the distribution channel).
- Structured OpenTelemetry tracing/metrics/logs with `gen_ai.*` semantic conventions; opt-in protected loopback Prometheus `/metrics` scrape endpoint.
- Mandatory addenda: **Operations & Lifecycle** (daemon state machine, socket ownership, SQLite contract, upgrade protocol, crash recovery, `zornmesh doctor`); **Compliance & Regulatory** (EU AI Act, NIST AI RMF, NIST SP 800-218A, GDPR, SOC 2, CISA SBOM); **Stakeholder Map**.
- Published `mesh-trace/1.0` open standard before public MVP.

**Explicit v0.1 NON-goals (deliberate "not yet"):** per-agent crypto identity · signed envelopes · capability tokens · multi-host federation · A2A bridge · AGNTCY/SLIM bridge · hosted/cloud dashboard · LAN/public web console · accounts/teams · rich chat workspace · workflow editor · dynamic policy · any replication · Windows · Python SDK · exactly-once delivery · total ordering across topics · embedded NATS · push-without-bounded-buffers · TLS on UDS · E2E encryption between agents.

### Growth Features (Post-MVP)

**v0.2 (~6 weeks after v0.1):**

- Per-agent **Ed25519** keypair identity (`~/.config/zornmesh/keys/<id>.ed25519`, mode 0600); signed envelopes for sensitive ops.
- **Windows named-pipe** support (`\\.\pipe\zornmesh-<sid>`) with SDDL/DACL strategy.
- **Python SDK** (`protobuf` runtime + Pydantic v2; `asyncio.timeout()` discipline).
- MCP-stdio bridge polish.
- Replay protection via per-envelope `id` + recent-ID set.
- **Python cancellation suite** running every PR (highest 2 a.m. risk).

**v0.5 (adoption and ecosystem expansion):**

- **Capability tokens** via `biscuit-auth` 5 with macaroons-style attenuation; daemon validates per operation; audit log records token serial; TTL ≤ 5 min.
- **Advanced UI expansion** for richer topology analysis, saved views, DLQ/replay workflows, and cross-session search. The v0.1 local UI remains the foundation; future UI work extends it rather than introducing the first browser surface.
- **A2A 1.0.0 gateway** for agent-peer RPC interop.
- Optional **`fjall` hot-log split** ONLY if measured write-stalls; SQLite stays for registry/leases/audit.

### Vision (Future) — v1.0+

- **Multi-host federation** between Zorn Mesh instances via a protected federation transport — same envelope and capability model survives the single-machine → multi-host transition (the upgrade path for the platform-engineer secondary persona).
- **Stable wire protocol** with formal deprecation policy.
- **AGNTCY/SLIM bridge** if demand emerges (threshold: 3 production deployments asking, or one paying customer).
- Default IPC fabric agents reach for when they need to coordinate on a single machine — **the SQLite of agent buses.**
- `mesh/*` namespace recognized alongside `tools/*` and `resources/*` as part of the protocol vocabulary developers learn first.
- Every major agent runtime ships a Zorn Mesh adapter out of the box.

## User Journeys

### Journey 1 — Maya, the Cursor user wiring three MCP servers (Primary, success path)

**Persona.** Maya is a senior product engineer at a 40-person startup. She lives in Cursor. Last month she wrote a custom MCP server that calls her company's internal docs API, and another that runs `pytest` on a feature branch. Today she wants to add a third agent — a Claude-powered code reviewer — and have all three coordinate: when she touches a file, the reviewer should pull context from the docs agent and run targeted tests. She has tried gluing them together with shell scripts and a JSON file in `/tmp`. It works on her laptop and only on her laptop, and only when the moon is right.

**Opening scene.** It's a Tuesday afternoon. Maya has just read the Zorn Mesh README on Hacker News. She has 20 minutes before her next meeting. Pain signature: three useful tools that cannot talk to each other; she's about to write yet another `subprocess.Popen` shim.

**Rising action.**

1. `cargo install zornmesh` — the binary lands in `~/.cargo/bin` in 90 seconds. No daemon yet.
2. `npm i @zornmesh/sdk` in her TypeScript project.
3. She opens the `examples/three_agents.ts` example, pastes 12 lines into her own project, replacing the placeholder agent IDs with `docs-server`, `pytest-runner`, `reviewer`.
4. First call to `Mesh.connect()` auto-spawns the daemon (she sees one log line: `"event":"ready","socket":"/run/user/1000/zornmesh/sock"`). No port to allocate, no Docker, no config file.
5. She wires the docs agent to publish on `agent.docs.search.>`, the pytest agent to subscribe to `agent.reviewer.test_request`, and the reviewer to `request` against both.
6. She runs the example. Three log lines fly past. Three agents have just had a conversation through her own machine.
7. She runs `zornmesh ui`; the browser opens to a loopback-only Live Mesh Control Room. The three agents appear without setup, each with status, capabilities, transport, and last-message recency.

**Climax.** She clicks the UUID printed by the reviewer in the local UI, then confirms the same story with `zornmesh trace <correlation_id>`. The Focus Trace Reader renders a complete timeline: reviewer → docs (pub/sub), reviewer → pytest (request/reply), pytest → reviewer (response, 412 ms), reviewer → Maya's editor (event). Every envelope, every hop, every timing. She did not configure observability. It was already on.

**Resolution.** Eight minutes from `cargo install` to first cross-agent message; ten minutes including the trace. Maya commits the example code. The next time someone on her team builds a multi-agent flow, she can hand them the same daemon binary and say "it just works." Her shell-script shim is deleted.

**Capabilities revealed.** Auto-spawn lifecycle · `Mesh.connect()` SDK API · agent registration with capabilities · pub/sub with hierarchical subjects · request/reply with `correlation_id` · `zornmesh trace` CLI · `zornmesh ui` local control plane · live agent roster · Focus Trace Reader · cross-language SDK parity (Rust + TS) · zero-config OpenTelemetry tracing.

### Journey 2 — Daniel, the on-call dev at 2 a.m. (Primary, edge case / forensic recovery)

**Persona.** Daniel maintains the multi-agent code-review pipeline that he and Maya built six months ago. Three agents now: docs lookup, test runner, reviewer. The pipeline runs whenever a PR is opened and posts a summary comment. It is part of the team's workflow.

**Opening scene.** 02:14 a.m., Wednesday. Daniel's phone vibrates with a Slack mention from a colleague in another timezone: "PR #1847 has been waiting 40 minutes for the bot, normally 2 minutes — is something stuck?" Daniel opens his laptop in the dark.

**Rising action.**

1. He opens `zornmesh ui` in his browser. The Live Mesh Control Room shows the reviewer agent stale and a recent dead-letter warning.
2. He clicks the `correlation_id` for PR #1847. The timeline rebuilds: reviewer published a request to `agent.docs.search.>` at 01:34:22.105, docs agent received it at 01:34:22.108, docs agent emitted a response at 01:34:22.461 — and then nothing. The response never reached the reviewer.
3. He opens Focus Trace Reader. The trace shows the reviewer agent's heartbeat went stale at 01:33:58 — six seconds *before* the request was published. The daemon still routed the response, but had no live consumer to deliver it to. The lease expired and the message moved to the dead-letter queue.
4. The UI shows the orphaned response with reason `consumer_unreachable` and offers copyable CLI recovery commands: `zornmesh inspect dead_letters --since 01:30`, `zornmesh agents`, and `zornmesh replay --correlation-id 7c4e9d-…`.
5. He restarts the reviewer agent. He runs the copied replay command — the response is re-delivered from the audit log. The PR comment posts within 30 seconds.

**Climax.** The recovery took 4 minutes total. Daniel did not have to dig through three separate log files, correlate timestamps by hand, or guess. The audit log was testimony. Every envelope was already there. He types `sqlite3 ~/.local/state/zornmesh/mesh.db "SELECT * FROM messages WHERE correlation_id=? ORDER BY ingress_us"` to confirm the SQL story matches the UI trace — first-class debug surface, no separate tooling.

**Resolution.** Daniel files an issue: "reviewer agent should have a supervised restart watchdog." He updates the team's runbook with the exact `zornmesh trace` and `zornmesh replay` commands. He goes back to bed at 02:18. The next morning he pastes the trace output into Slack and the team understands what happened in 30 seconds — no one says "looks like it just glitched."

**Capabilities revealed.** Persistent audit log with `correlation_id` indexing · Focus Trace Reader and `zornmesh trace` reconstructing across multiple agents · heartbeat staleness detection · lease expiry → DLQ contract · guided recovery panel with copyable CLI commands · `zornmesh inspect dead_letters` · `zornmesh agents` (live registry) · `zornmesh replay --correlation-id` · SQLite-as-debug-surface (the `sqlite3` query is first-class, not a workaround) · DLQ reason codes (`consumer_unreachable`).

### Journey 3 — Priya, the Claude Desktop user joining the bus via stdio bridge (Integration, launch-gate forcing function)

**Persona.** Priya is an applied researcher. She lives in Claude Desktop. She has a custom Python research agent that pulls papers from arXiv, and she's heard her teammate Maya talk about Zorn Mesh. She wants Claude Desktop to be able to call her arXiv agent — but she does not want to modify Claude Desktop, and she does not want to maintain a custom MCP server.

**Opening scene.** Friday, late afternoon. Priya has installed the daemon (`cargo install zornmesh`) and started her Python arXiv agent (using the Rust SDK via subprocess at v0.1; native Python SDK lands in v0.2). She opens Claude Desktop and adds an MCP server entry to its config: `command: zornmesh stdio --as-agent claude-desktop`.

**Rising action.**

1. She restarts Claude Desktop. The MCP handshake completes — Claude Desktop sees Zorn Mesh as a regular MCP server with a small set of `mesh/*` tools (publish, subscribe, request).
2. She types into Claude Desktop: "Find me the latest papers on diffusion-transformer scaling." Claude calls `mesh/request` against `agent.arxiv.search`. The arXiv agent receives the request, fetches results, returns them.
3. The response renders in Claude Desktop as if it had come from a native MCP tool. Priya did not write a single line of MCP server code. She did not modify Claude Desktop.
4. She opens `zornmesh ui` and sees `claude-desktop` join the mesh as a stdio-connected agent, with the arXiv agent shown as the request target.

**Climax.** She clicks the `correlation_id` in the local UI and sees the bridge: `claude-desktop` → `agent.arxiv.search` → response. The stdio bridge is compatible with MCP 2025-11-25 — the same protocol family her Claude client already speaks. She had assumed she would have to write an MCP server. She did not.

**Resolution.** Priya pings her team's #ai-tooling channel: "Anyone with Cursor or Claude Desktop can talk to my arXiv agent now via Zorn. No code changes on your end." Adoption wedge consumed: the MCP-stdio bridge made an existing MCP host a participant on the bus without asking the host vendor for anything.

**Capabilities revealed.** `zornmesh stdio --as-agent <id>` bridge subcommand · MCP 2025-11-25-compatible handshake · `mesh/*` reserved namespace (no collision with `tools/*`, `resources/*`, etc.) · symmetric capability advertisement at handshake · local UI visibility for bridged hosts · cross-host trace continuity (correlation_id flowing through the bridge).

### Journey 4 — Sam, the platform engineer doing v0.5 due-diligence (Secondary persona, post-MVP)

**Persona.** Sam is a platform engineer at a 600-person company. The CTO has asked him to "figure out our agent platform story" before the next planning cycle. He is going to evaluate three to five options. He cares about: lock-in, observability, security posture, multi-host upgrade path, compliance (SOC 2, EU AI Act).

**Opening scene.** Sam has a 4-hour evaluation window for each candidate. He starts Zorn Mesh's by running `cargo install zornmesh` on his work laptop, the way one of his developers would. The bus is up in 90 seconds.

**Rising action.**

1. He runs `zornmesh doctor` — a dedicated diagnostics command reports daemon version, socket path, SQLite schema version, OS-level trust posture (UID match active, socket mode 0600 in dir 0700), OpenTelemetry collector reachability, and any pending migrations.
2. He runs `zornmesh ui --print-url`; the URL is loopback-only and session-token protected. The browser shows daemon trust posture, connected agents, local-only status, and no external asset fetches.
3. He reviews the published `mesh-trace/1.0` open standard. Conversation replay is documented as a vendor-neutral spec, not a Zorn-only feature. Lock-in concern: drops.
4. He reads the Compliance & Regulatory PRD addendum. EU AI Act provisions, NIST AI RMF mapping, NIST SP 800-218A SSDF posture, GDPR data-handling (audit log retention is explicit, configurable, and SQLite-local), SOC 2 control mapping, CISA SBOM published per release. He runs `zornmesh inspect sbom` — the SBOM for his installed binary is right there.
5. He reads the v1.0 federation roadmap: protected federation between Zorn Mesh instances on different machines, same envelope and capability model, no architectural rework. The single-machine prototype his developers build today survives the multi-host transition.
6. He runs the three-agent example, then asks one of his senior developers to do the same. Both reach first-coordinated-message in under 10 minutes and both can explain the trace from the local UI without hand-stitching logs.

**Climax.** Sam writes a one-page evaluation memo. The line his CTO underlines: *"The local-first prototype shape is the same as the production multi-host shape. Our developers learn one mental model. Compliance posture is documented and machine-checkable. No new infrastructure to operate at v0.1."*

**Resolution.** Sam recommends adoption as the standard local IPC fabric for the org's agent prototypes, with a v0.5 re-evaluation gate for the federation upgrade path. His developers start using it the next week. Sam himself never has to operate a broker — the daemon is the developers' problem, on the developers' machines.

**Capabilities revealed.** `zornmesh doctor` self-diagnostic · `zornmesh ui --print-url` local trust/status view · `zornmesh inspect sbom` · published `mesh-trace/1.0` open standard · Compliance & Regulatory addendum (EU AI Act, NIST AI RMF, NIST SP 800-218A, GDPR, SOC 2, CISA SBOM) · documented forward-only migration story · clear single-machine → multi-host federation upgrade path · Stakeholder Map identifying enterprise security reviewers and platform engineers as recognized stakeholders.

### Journey 5 — Safe direct/broadcast message from the local control plane (Core intervention flow)

**Persona.** Maya or Daniel has a live mesh with 3 agents and needs to intervene without opening three terminals or guessing which process should receive the instruction.

**Opening scene.** The Live Mesh Control Room shows `docs-server`, `pytest-runner`, and `reviewer`. A trace reveals the reviewer is waiting for test context, and the developer wants to ask the pytest agent to re-run a focused test, then broadcast a "pause non-essential work" instruction to all agents.

**Rising action.**

1. The developer selects `pytest-runner` and opens the safe composer.
2. The UI shows target agent, capability summary, payload preview, and whether the send will be direct or broadcast.
3. For direct send, the developer confirms once; the UI shows queued → delivered → acknowledged state.
4. For broadcast, the UI requires recipient preview and explicit confirmation. Recipients with incompatible capabilities are excluded or marked before send.
5. After send, the UI shows per-recipient outcomes, partial failures, and a copyable equivalent CLI command for repeatable automation.

**Climax.** The developer sees exactly which agents received the instruction, which declined or failed, and which trace/correlation ID now contains the intervention. Broadcast feels safe because ambiguity was removed before send.

**Resolution.** The intervention is auditable, replayable, and understandable from both UI and CLI. The developer did not create an invisible side channel; they used the same broker semantics as every agent.

**Capabilities revealed.** Safe composer · direct send · broadcast recipient preview · explicit confirmation · per-recipient outcomes · delivery state badges · trace correlation for human-originated messages · CLI handoff copy block · audit-log entry for human intervention.

### Journey Requirements Summary

| Capability area | Drawn from journeys | Surface (CLI / SDK / UI / spec) |
|---|---|---|
| Auto-spawn daemon lifecycle | J1, J3, J4 | SDK `Mesh.connect()`, daemon `start`/`stop`, PID/socket discipline |
| Agent registration + capability advertisement | J1, J2, J3 | `mesh/register`, AgentCard handshake, `zornmesh agents` |
| Pub/sub with hierarchical subjects + wildcards | J1, J2 | `mesh/publish`, `mesh/subscribe`, `*` and `>` wildcards |
| Request/reply with correlation + cancellation | J1, J2, J3 | `mesh/request`, `mesh/fetch`, `mesh/ack`, `cancel` control frame |
| At-least-once delivery + idempotency + leases + DLQ | J2 | mandatory `idempotency_key`, lease reaper, `dead_letters` table |
| Persistent audit log with offset replay | J1, J2 | append-only SQLite `messages`, `zornmesh replay --correlation-id` |
| Forensic CLI surface | J1, J2 | `zornmesh trace`, `tail`, `inspect`, `agents`, `replay` |
| Local web control plane | J1, J2, J3, J4, J5 | `zornmesh ui`, Live Mesh Control Room, Focus Trace Reader |
| Safe human intervention | J5 | direct send, broadcast preview/confirmation, per-recipient outcome list |
| MCP-stdio bridge (launch gate) | J3 | `zornmesh stdio --as-agent <id>`, MCP 2025-11-25 byte-compat, `mesh/*` reserved namespace |
| Symmetric capability model | J3 | both ends advertise consumer + provider sets at handshake |
| OS-level trust (`SO_PEERCRED` UID match + socket ACL) | J4 | UDS `chmod 0600` in dir `chmod 0700`, no abstract sockets |
| Diagnostics + self-checks | J4 | `zornmesh doctor`, `zornmesh inspect sbom`, migration version reporting |
| Compliance & Regulatory posture | J4 | published Compliance addendum, SBOM per release, machine-checkable controls where possible |
| Open standard (`mesh-trace/1.0`) | J3, J4 | published before public MVP, `zornmesh trace` is reference impl |
| Forward-only migration story | J4 | `/migrations/NNNN_*.sql`, applied at startup, CI gate |
| OpenTelemetry tracing/metrics/logs | J1, J2 | `gen_ai.*` semantic conventions, opt-in protected loopback `/metrics` scrape, W3C tracecontext as first-class envelope members |
| Cross-language SDK parity (Rust + TS at v0.1; Python at v0.2) | J1, J3 | `crates/zornmesh-sdk`, `@zornmesh/sdk`, future `zornmesh-py` |
| Federation upgrade path (vision) | J4 | v1.0 protected federation transport, same envelope/capability model |

## Domain-Specific Requirements

The locked classification names this product as `developer_infra` (informal taxonomy) with `complexity = high` and `general` as the BMad-CSV fallback for tooling that reads the standard taxonomy. The high-complexity classification is justified by three properties unique to AI-agent IPC fabric:

- **It is the audit boundary.** Every cross-agent message — including LLM tool calls and prompt-laden envelopes — flows through the daemon's append-only log. The audit log is the answer to "what did the agents actually do," which is also the answer regulators are increasingly going to ask.
- **It is a privileged-by-uid local broker.** The threat surface is small but every component on the surface (UDS, SQLite, signed envelopes, capability tokens) is load-bearing for trust.
- **It sits in the AI compliance fan-out.** EU AI Act, NIST AI RMF, NIST SP 800-218A SSDF, GDPR (data minimization for prompts), SOC 2 (audit-trail integrity), and CISA SBOM each touch this product through the audit log, the SBOM, or the security model.

## Compliance & Regulatory

Each item below is **machine-checkable** (CI gate or release gate) unless explicitly marked otherwise.

| Framework | Applies because | v0.1 posture | Evidence artifact |
|---|---|---|---|
| **EU AI Act** (Reg. (EU) 2024/1689) | Zorn Mesh is component infrastructure for general-purpose AI systems. It is not itself a high-risk AI system, but it carries audit obligations for downstream high-risk deployments that route through the bus. | Append-only `audit_log` with hash-chained tamper-evidence (Cat 4); per-envelope `trace_id`/`correlation_id` retained for the full audit window; configurable retention; `audit_log` is **never truncated** within the 30-day default window. Operators in EU-deployed downstream high-risk contexts can configure longer retention. | Published "EU AI Act mapping" doc citing specific Articles (esp. Art. 12 record-keeping, Art. 15 robustness/cybersecurity); `zorn audit verify` offline-replay tool. |
| **NIST AI RMF (AI 100-1)** | Voluntary US framework that downstream deployers adopt; Zorn Mesh enables Govern/Map/Measure/Manage functions through the audit log + tracing. | Trace continuity (W3C tracecontext as first-class envelope members), machine-readable trace export, cardinality discipline so traces survive at scale, named DLQ reason codes. | "NIST AI RMF mapping" doc; conformance fixtures under `/conformance/observability/`. |
| **NIST SP 800-218A** (SSDF for AI software) | Applies to the Zorn Mesh codebase itself as AI-adjacent infrastructure. | Lockfile discipline (`Cargo.lock`, `bun.lock`, `uv.lock`); `cargo audit` + JS dependency audit + `uv pip audit` as **machine-enforced CI gates**; signed releases (Sigstore/cosign); reproducible builds where toolchain permits; coordinated vulnerability disclosure policy in `SECURITY.md`. | CI gate logs; published `SECURITY.md`; release signing manifest. |
| **GDPR** (data minimization, lawful basis for processing) | Prompts and tool arguments routed through the bus may contain personal data the operator did not intend to persist. | **Configurable retention** (24 h messages, 7 d DLQ, 30 d audit by default); operators can shorten retention without breaking audit-log integrity (audit retention is independent of message retention); explicit data-handling section in operator docs; payload encryption at rest is **out of scope at v0.1** (kernel protects loopback IPC; documented). | Operator-facing data-handling document; retention configuration in `zornmesh doctor` output. |
| **SOC 2 Type 2** (downstream adopter requirement) | Adopters running Zorn Mesh in SOC-2-scoped environments need evidence that audit-trail integrity is preserved by the daemon. | Tamper-evident hash-chained `audit_log`; offline verification (`zorn audit verify`); audit entries written for every authorization decision (permit AND deny) on capabilities tagged `requires_audit = true`; trust-level transitions, key rotations, registrations, capability changes all audit-logged. | Conformance fixtures under `/conformance/security/`; published "SOC 2 control mapping" doc. |
| **CISA SBOM** (per Executive Order 14028 and successors) | Zorn Mesh distributes a single static binary. Adopters need a per-release SBOM. | **CycloneDX SBOM published per release**; `zornmesh inspect sbom` returns the SBOM for the installed binary; SBOM generated by `cargo-cyclonedx` (Rust) and `cyclonedx-npm` (TS SDK); CI fails the release if SBOM is missing or stale relative to lockfile. | Release artifact: `sbom.cdx.json` per binary; `zornmesh inspect sbom` CLI. |

**Out-of-scope at v0.1, named for traceability:**

- **HIPAA / PCI-DSS / FINRA**: Zorn Mesh is general-purpose IPC; it does not itself process PHI or cardholder data. Adopters in those domains carry the obligation, and the audit-log + retention-configurability gives them what they need. Documented in operator guide; **not** marketed as a HIPAA/PCI solution.
- **FedRAMP / IL2+**: out of scope at v0.1 and v0.5. Single-machine local-first architecture is poorly aligned with FedRAMP boundary expectations until the v1.0 federation layer lands.
- **ITAR / EAR (export controls)**: the cryptographic primitives (Ed25519, biscuit-auth) are commercially available open-source; Zorn Mesh is open source. Standard OSS posture; no special export-control program.

## Operations & Lifecycle

This addendum is the operator-facing contract for the local daemon, not the implementation design. It consolidates lifecycle semantics that appear elsewhere in FRs/NFRs so downstream architecture, runbooks, and stories use one source of truth.

### Daemon State Model

The daemon exposes a small, observable state model:

| State | Meaning | Required operator signal |
|---|---|---|
| `starting` | Process launched; runtime config, data dir, socket path, schema version, and privilege posture are being checked. | Bootstrap log plus `zornmesh doctor` reports startup phase if queried. |
| `ready` | Socket owned, schema current, registry available, and routing loop accepting connections. | `zornmesh doctor` returns healthy status and the UI trust panel shows current. |
| `degraded` | Read paths and diagnostics remain available, but a bounded condition prevents normal writes or sends. | Named degraded reason, remediation text, and audit/health event. |
| `draining` | Shutdown requested; no new work accepted; in-flight work drains within configured budget. | Exit summary includes drained count, DLQ count, and reason. |
| `stopped` | Daemon released owned resources and no longer accepts local mesh traffic. | CLI status reports not running without treating absence as corruption. |

Transitions into `degraded` and `draining` must be explicit and diagnosable. Silent fallback to partial behavior is out of scope.

### Socket Ownership and Local Trust

Exactly one daemon owns the local mesh socket for a user/session. SDK auto-spawn, explicit `zornmesh daemon`, and `zornmesh ui` all converge on the same ownership contract: no second daemon may steal an active socket, and diagnostics must explain stale socket recovery without requiring users to inspect runtime directories manually. Socket ownership remains tied to the invoking user and loopback/local IPC trust model; public or cross-user exposure is out of scope at v0.1.

### SQLite and Audit Contract

The local store is a single-user persistence and audit boundary. Messages, dead letters, idempotency records, audit entries, schema version, and retention state remain queryable through daemon/CLI/UI surfaces; SDKs do not open the store directly. Forward-only migrations are the only schema upgrade path. Retention must preserve audit integrity, and recovery must prefer explicit degraded/refusal states over silent data loss.

### Upgrade and Crash Recovery Protocol

An upgrade must either leave the daemon at the previous usable schema or complete with the new schema and a passing smoke check. Crash recovery must restore a diagnosable state: committed acknowledged envelopes remain replayable, in-flight work is either resumed or moved to a named recovery state, and duplicate work is bounded by idempotency semantics. Operators must be able to confirm recovery with `zornmesh doctor`, `zornmesh trace`, and replay/audit commands without reading raw database internals.

### `zornmesh doctor` Diagnostic Contract

`zornmesh doctor` is the canonical lifecycle diagnostic. It reports daemon status, socket path/ownership, schema version, migration state, release signature/SBOM visibility, OTel reachability, UI loopback/session posture when active, data-dir writability, retention/degraded state, and remediation hints. Machine-readable output is required for CI and support scripts.

## Stakeholder Map

Zorn Mesh has one v0.1 buyer/user overlap (the individual developer) and several adoption influencers. This map makes those stakeholder needs explicit so epics and docs do not optimize only for the happy-path developer demo.

| Stakeholder | Role in adoption | Primary need | v0.1 success signal | Risk if missed |
|---|---|---|---|---|
| Individual developers | Primary user and evaluator | First coordinated message, trace comprehension, safe local intervention | Runs example, sees ordered trace, can recover or send safely without infrastructure | Product feels like another broker to operate |
| Platform engineers | Future buyer/influencer | Local prototype shape that can grow into governed fleet patterns | Can evaluate trust posture, upgrade path, audit model, and federation roadmap | Blocks standardization as a toy/dev-only tool |
| Enterprise security reviewers | Gatekeeper for managed environments | SBOM, signatures, local-only boundaries, privilege refusal, auditability | Can verify release artifacts, trust model, retention, and no public listener | Blocks internal adoption due unclear threat surface |
| Compliance reviewers | Evidence consumer | Traceable audit records and mapping docs for downstream regulated deployments | Can export evidence and understand EU AI Act/NIST/GDPR/SOC 2 posture | Compliance claims look hand-wavy or unverifiable |
| Agent-runtime vendors | Ecosystem amplifier | Low-friction bridge/adapter path that does not fork host behavior | MCP-stdio bridge works; adapter story is clear for v0.5 | Hosts treat Zorn as competing infrastructure instead of an interop layer |
| Protocol stewards | External standard alignment | MCP/A2A compatibility without namespace collision or version drift | Compatibility docs and conformance fixtures are visible | Protocol drift weakens adoption wedge |
| OSS contributors | Implementation and trust multiplier | Clear boundaries, fixtures, docs, and issue labels | Can find contribution area without reverse-engineering architecture | Contributions skew toward scope creep or rewrites |
| UX/accessibility reviewers | Local UI quality gate | Calm, inspectable, keyboard/screen-reader-safe control surface | Core UI fixtures cover first open, trace inspection, safe send, reconnect, and keyboard paths | UI becomes confusing or unsafe despite good backend semantics |

Engagement priority at v0.1 is: individual developer proof, platform/security confidence, and protocol compatibility. v0.5 expands adapter-vendor and platform-engineer validation once the local witness/control loop is proven.

## Technical Constraints

**Security model (locked, see project-context §Security Model):**

- **Trust anchor at v0.1: kernel `SO_PEERCRED` UID match** + socket ACL (`chmod 0600` on UDS in `chmod 0700` directory). Abstract Unix sockets on Linux are explicitly **rejected** (no filesystem ACL).
- **Daemon = trust boundary.** The daemon trusts the kernel's `(uid, gid, pid)` for the connecting socket. Envelope-claimed identities are validated against kernel-reported identity at registration; mismatch → `NACK` and connection close.
- **AgentCard.id stability** is mandatory across restarts. Re-registration from a different `(uid, gid)` triggers a `trust_level_changed` audit event and downgrades the agent until operator reapproval.
- **Threat model explicitly out of scope:** kernel compromise, root-on-host adversary, supply-chain attack on the Rust/TypeScript/Python toolchains (mitigated by CI audit gates, not runtime detection), TLS on UDS (pointless), end-to-end encryption between agents on the same host (kernel protects loopback IPC).
- **v0.2 layer:** per-agent Ed25519 keypair identity; signed envelopes for high-sensitivity capabilities; nonce + anti-replay window enforced by daemon.
- **v0.5 layer:** capability tokens via `biscuit-auth` 5 with macaroons-style attenuation; TTL ≤ 5 min; daemon validates per-operation; audit log records token serial; revocable mid-session.

**Local web UI security model (v0.1):**

- The UI is opened explicitly by `zornmesh ui`; it never starts a public listener as a side effect of SDK connect.
- Browser access is loopback-only (`127.0.0.1` / `::1`) with a per-session token, origin checks, and CSRF protection on state-changing operations.
- Binding to `0.0.0.0`, LAN interfaces, or a public hostname is out of scope at v0.1; any future remote console requires a new threat model and explicit change control.
- Static UI assets are bundled locally with the release artifact; no CDN, remote font, analytics, auto-update, or runtime remote config fetch is permitted.
- The browser never reads SQLite directly. UI data comes through daemon-owned APIs that enforce the same redaction, retention, and audit rules as CLI/SDK access.
- Human-originated direct and broadcast sends are audit-logged with actor/session, recipient set, trace/correlation ID, payload summary, and per-recipient outcome.

**Privacy and data handling:**

- Audit log entries do **not** include payload bytes; payloads live in `messages` table and follow message retention.
- High-cardinality identifiers (`message_id`, `correlation_id`, `trace_id`, `idempotency_key`) are span attributes only — **never** metric labels (Cat 6, machine-enforced via `ZORN_METRICS_MAX_LABEL_VALUES` cardinality cap, default 10000).
- Secrets in code use a `Secret<T>` wrapper that overrides `toJSON`/`toString`/`Symbol.toPrimitive`/`Display` to redact (machine-enforced via custom lints).

**Performance (real-time, not soft-real-time):**

- Daemon cold-start ≤ 200 ms (Cat 1).
- Writer transaction budget ≤ 50 ms anchored at inbox dequeue (Cat 3).
- Streaming backpressure: 256 KiB outstanding-bytes window per stream (matches HTTP/2 / gRPC flow control patterns; the previous 64-chunk cap was incompatible with token-streaming).
- Dedupe window default 5 s, configurable per capability up to 300 s.
- TTL/retry interaction: retries never deliver past TTL; expired retries → DLQ.

**Availability:**

- Single-machine, single-binary, single-SQLite-file. **No HA, no replication at v0.1, v0.5, or v1.0** — these are explicit non-goals (would be a different product). Adopters needing HA run their workloads on multiple developer machines, each with its own daemon.
- Graceful shutdown budget 10 s default (`ZORN_SHUTDOWN_BUDGET_MS`, max 60 s). Drains pending NACKs before exit.
- Crash recovery: SQLite WAL replay on restart; pending leases re-emerge for redelivery; idempotency keys prevent duplicate work.

## Integration Requirements

**Wire compatibility (load-bearing):**

- **MCP 2025-11-25** byte-compatible handshake. Zorn-specific methods live under reserved `mesh/*` namespace; never collide with MCP's `tools/*`, `resources/*`, `prompts/*`, `sampling/*`, `roots/*`, `elicitation/*`.
- **A2A 1.0.0** — gateway adapter at v0.5.
- **AGNTCY / SLIM** — v1.0+ bridge if demand emerges (threshold: 3 production deployments asking, or one paying customer).
- **ANP** — research curiosity, not production.

**Identity / capability schema:**

- `AgentCard` document from A2A 1.0.0 used **verbatim** as the agent identity payload. The daemon's in-memory registry is keyed by `AgentCard.id`. Capability advertisement reuses A2A's capability descriptor structure rather than inventing a new IDL.
- Symmetric capability model (vs MCP's hub-and-spoke): both ends of every connection advertise both consumer and provider capabilities at handshake.

**Observability (mandatory, not optional):**

- **OpenTelemetry only** for traces/metrics/logs. No vendor-specific SDKs (Datadog, New Relic, Honeycomb agents) anywhere in the core.
- W3C tracecontext (`trace_id`, `span_id`, `parent_id`) as first-class envelope members; daemon propagates across every routing hop.
- Protected loopback Prometheus `/metrics` scrape endpoint on the gateway when explicitly enabled; local-first observability cannot require a hosted collector.
- `gen_ai.*` semantic conventions for any LLM-touching span (still experimental in SemConv v1.37 as of 2026-04 but the de-facto target).

**Schema / wire stability:**

- Protobuf is the canonical schema (`proto/zorn/mesh/v0/`); JSON is the canonical wire (Buf canonical-JSON mapping makes them interchangeable).
- `buf breaking` runs in CI against the previous tag — wire-stability gate.

## Risk Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **Audit log retention wiped before incident discovery** | Medium | High (compliance breach) | Audit log is **never truncated** within the configurable window (default 30 d); `zorn audit verify` offline replay; operators in regulated environments configure longer retention; documented in operator guide. |
| **Tamper attempt on audit log (rogue agent or compromised host)** | Low | High | Hash-chained tamper-evident entries; `zorn audit verify` detects breaks; entries pinned by daemon, not agents; on detection, `audit_chain_break` event raised. |
| **Bootstrap-deadlock on partial-handshake failure** (Dr. Quinn open question) | Low | Medium | Property-test exploration before v0.1 lock; named handshake-failure error codes; explicit handshake timeout (≤ 1 s); fallback path. **Open question — flagged for resolution before v0.1 ship.** |
| **Trace/metric cardinality explosion** | Medium | High (collector crash) | Cardinality cap (`ZORN_METRICS_MAX_LABEL_VALUES`, default 10000); high-cardinality attrs (`message_id`, `trace_id`, `correlation_id`, `idempotency_key`) belong in spans only, **never** metric labels (machine-enforced); `metrics.cardinality_exceeded` warning span on cap hit. |
| **SQLite write-stall under sustained load** | Medium | Medium (latency degradation) | 50 ms transaction budget anchored at inbox dequeue (visible in latency percentiles); single writer connection (multi-writer SQLite adds zero throughput, complicates locks); `fjall` hot-log split is a measured-evidence escape hatch reserved for v0.5 if real, NOT premature. |
| **Compromised agent process exfiltrating via the bus** | Low (v0.1) → Medium (post-MVP) | High | At v0.1, UID-match trust limits scope to user-owned processes. At v0.2, signed envelopes for sensitive ops. At v0.5, capability tokens with short TTL, revocable mid-session, audit-logged on issue and use. |
| **Supply-chain attack on Rust / TypeScript / Python toolchains** | Low | High | `cargo audit` + JS dependency audit + `uv pip audit` as machine-enforced CI gates; lockfiles committed; signed release artifacts; per-release CycloneDX SBOM; coordinated vulnerability disclosure in `SECURITY.md`. **Runtime detection is explicitly out of scope** — adopters layer their own EDR. |
| **Schema drift breaking SDK consumers** | Low | High | `buf breaking` CI gate against previous tag; per-SDK `build_version` drifts independently from `protocol_version`; semver discipline; deprecation policy formalized at v1.0. |
| **Operator runs daemon as root or with elevated privilege "to be safe"** | Medium | High (unnecessary attack surface) | Daemon refuses to start as root with named error; `zornmesh doctor` flags privileged execution; documented in operator guide. |
| **Local UI accidentally exposed beyond loopback** | Low | High | `zornmesh ui` binds only to loopback, refuses non-loopback addresses at v0.1, uses a session token, and exposes local trust status in the UI chrome. |
| **Stale UI state causes wrong diagnosis** | Medium | High | Every UI event carries daemon sequence and receipt time; reconnect performs backfill before declaring the view current; stale/disconnected states are visible and never color-only. |
| **Unsafe broadcast sends to unintended agents** | Medium | High | Broadcast requires recipient preview, explicit confirmation, incompatible-recipient warnings, per-recipient outcomes, and an audit record linked to the originating trace. |
| **Federation marketed before ready** (v1.0+) | Low (governance) | Medium (trust erosion) | Roadmap explicitly marks v1.0 federation as a future feature; v0.1 messaging is local-first; "single-machine by design" appears in README, brief, and PRD. |

## Innovation & Novel Patterns

### Detected Innovation Areas

The innovation in Zorn Mesh is **not** a single invented idea — it is the **deliberate three-property compound** that no current product assembles. The strategic claim is paradigm-shift through pattern transfer, not novelty for its own sake.

**1. The compound positioning: "the SQLite of agent buses."** Three independently validated patterns, fused at the developer-machine scale:

- **MCP is the wire** (not a translation layer): byte-compatible with MCP 2025-11-25 at the framing and namespace level. Existing MCP clients are first-class participants.
- **NATS is the model** (not a dependency): hierarchical subjects, `*` and `>` wildcards, durable subscriptions, lease-based pull delivery — proven at planet scale, applied to one machine.
- **SQLite is the store** (not a hot-log database): single file, single writer, WAL-mode, debuggable with `sqlite3` from the shell. The 2 a.m. forensic story is `sqlite3 mesh.db 'SELECT …'`.
- **Rust is the runtime** (not a Go-runtime daemon): 22 MB single static binary, 5–15 MB RSS, no GC pause stories.

Any two of those properties and the product is a developer tool. **All three at once is the category claim.**

**2. The MCP-superset wire (no opt-in friction).** The architectural choice that makes Zorn Mesh *not* an MCP replacement, but a **superset that any MCP client can ride for free**, is genuinely novel. Specifically: every Zorn-specific method lives under the reserved `mesh/*` namespace and never collides with MCP's `tools/*`/`resources/*`/`prompts/*`/`sampling/*`/`roots/*`/`elicitation/*`. An MCP client that only cares about tools sees Zorn Mesh as a regular MCP server with a small set of mesh-management tools. Adoption wedge: `zornmesh stdio --as-agent <id>` makes existing Claude Desktop / Cursor / VS Code Copilot users participants on the bus without modifying the host.

**3. The symmetric capability model.** Unlike MCP's hub-and-spoke (host = client, server = provider), every Zorn Mesh connection advertises **both consumer and provider capabilities at handshake**. This is the structural property that makes peer-to-peer agent comms feel native rather than tunneled. Re-using A2A 1.0.0 `AgentCard` as the identity payload (verbatim) means Zorn Mesh inherits A2A's capability descriptors rather than inventing a third IDL.

**4. The auto-spawn daemon for agent IPC (`sccache` pattern transfer).** Applying the `sccache --start-server` / Tauri sidecar / VS Code language client pattern to **agent messaging** is the architectural transfer. The library calls `try_connect → spawn_daemon_if_absent → retry_connect`; first SDK connect bootstraps the daemon. No operator. No `systemd` unit. No port allocation. The pattern is empirically validated in adjacent systems (X11, Docker socket, language servers, Litestream, Tauri, sccache) but has not been applied to agent IPC before this product.

**5. `mesh-trace/1.0` — a published open standard, not a proprietary feature.** Multi-agent conversation replay (correlation IDs, causality chains, message boundaries, agent identities, replay format) is published as a vendor-neutral spec **before** public MVP. `zornmesh trace` is the reference implementation. The bet: when "how do I audit my agent mesh?" becomes the universal question (~6 months from launch), the answer should already have a name. **Conceptual moat through standards leadership**, not feature lock-in.

**6. The local witness control room.** The Live Mesh Control Room makes the invisible local mesh legible: connected agents, message chronology, daemon trust status, and safe intervention live in one offline browser surface. It borrows the calm triage posture of Sentry, the speed/density of Linear, and the local runtime visibility of Docker Desktop, but remains a local developer-machine companion rather than a hosted dashboard.

**7. The forensic experience as paradigm shift.** "Jaeger-quality forensics with zero infrastructure setup" via `zornmesh trace <correlation_id>` and the Focus Trace Reader inverts the typical observability trade: distributed-tracing-grade output with no collector to run, no hosted UI, no service mesh to install. The novelty is **removing the cost of observability**, not adding a new observability format.

### Market Context & Competitive Landscape

| Adjacent product | What it solves | What it does NOT solve |
|---|---|---|
| **NATS** | Cross-machine messaging, planet-scale | Requires Go runtime + sidecar lifecycle; not local-first; no MCP wire |
| **Redis Pub/Sub** | Cross-process messaging | Network broker; no audit log; no agent identity model |
| **MCP servers (per host)** | Tool-calling within one host | Hub-and-spoke; no peer-to-peer; no replay; one host per server |
| **A2A 1.0.0** | Agent-to-agent RPC over network | Network protocol; assumes well-known peers; not local-first |
| **AGNTCY/SLIM** | Federated agent overlay (gRPC + MLS) | Heavy; quantum-safe focus; far ahead of current adoption |
| **Custom shell scripts + JSON files in /tmp** | Today's coping behavior | Brittle, opaque, no audit, no trust model — the universal pain |
| **Embedded NATS server** (e.g., synadia/nats embedded) | NATS in-process | Pulls in Go runtime + cgo boundary; not "boring"; rejected explicitly |
| **dbus / Mach ports** | Local IPC | OS-specific; no agent identity model; no audit log; not portable to MCP wire |

The **competitive empty quadrant** is "local-first + MCP-superset + audit-grade forensics + single static binary." No 2026 product occupies it.

### Validation Approach

**The launch-gate forcing function** is the validation: at v0.1, the MCP-stdio bridge must interoperate with **at least one major host** (Claude Desktop or Cursor) on a released build. If the bridge does not work end-to-end, the compound claim is unproven. There is no "ship without validating this."

**Hard-evidence validation gates:**

1. **MVP correctness gates** (already enumerated in Technical Success): codec round-trip property tests, 10,000 simulated agent crashes via `turmoil`, migration stress test, complete trace, browser E2E, UI security, accessibility, coverage gates, and `buf breaking`. **All nine must pass.**
2. **Three-agent example is production-grade**, not toy. `examples/three_agents.rs` and `examples/three_agents.ts` are CI-tested every PR; they ARE the user's first experience.
3. **`mesh-trace/1.0` open standard** published before public MVP, with at least one external review (preferably from an A2A working group or OpenTelemetry SIG contributor) before the spec freezes.
4. **Local UI browser E2E** proves the core UX loop: observe agents → inspect ordered trace → send safely → confirm outcome.
5. **Time-to-first-coordinated-message telemetry** (opt-in) measures the median and lets us tighten or loosen the < 600 s North Star based on real data, not aspiration.
6. **Adopter validation at v0.5:** ≥1 major agent runtime ships an adapter in default config; ≥1 production A2A bridge deployment outside the core team. If neither lands within ~6 months of v0.5, the category-creation thesis is wrong and the product re-positions as a niche developer tool.

**Soft-evidence validation:**

- "First-time-paste-trace-id-and-watch-rebuild" reaction from external testers (qualitative, but pattern-matched against the team's pre-tested theory of the killer moment).
- Whether "the SQLite of agent buses" survives one round of Hacker News critique without being mocked into oblivion (positioning sniff test).
- Whether enterprise security reviewers (Sam-persona) read the Compliance & Regulatory addendum and **don't ask for more** — i.e., the doc is sufficient on first read.

### Risk Mitigation

| Innovation Risk | If validation fails… | Fallback |
|---|---|---|
| **MCP wire compatibility breaks under real Claude Desktop / Cursor releases.** | Bridge cannot hold the launch-gate promise. | Maintain a tested matrix of (host version × Zorn Mesh version) pairs; `zornmesh stdio` exposes a `--mcp-version` flag for explicit pinning; hard launch-blocker on bridge regression. |
| **MCP wire format drifts away from current spec** (the "MCP roadmap surprises us" scenario). | Byte-compat claim erodes. | `mesh-trace/1.0` is the wire-independent fallback identity; the conceptual moat survives even if the byte-compat tactic has to evolve. The compound positioning ("SQLite of agent buses") doesn't depend on any single wire. |
| **Symmetric capability model confuses MCP-trained users** ("why is my server advertising consumer caps?"). | Adoption friction at the SDK boundary. | TypeScript and Rust SDKs default to provider-only registration; symmetric mode is opt-in with clear docs; adapter authors get a separate playbook. |
| **`mesh-trace/1.0` standard is ignored or co-opted by a competing spec** (e.g., A2A working group publishes their own first). | Conceptual moat collapses. | Engage the A2A working group and OpenTelemetry SIG **before** public MVP; if a co-opting spec emerges, propose Zorn's spec as a profile of theirs rather than fighting it. The reference implementation (`zornmesh trace`) is the durable advantage either way. |
| **The `sccache` auto-spawn pattern surprises adopters in production** (e.g., orchestrators that don't expect process forks). | Adoption blocker for CI runners and headless deployments. | `ZORN_NO_AUTOSPAWN=1` opt-out is supported from v0.1; documented; `zornmesh daemon start` explicit-mode is first-class. Production adopters use explicit-start; auto-spawn stays the developer-laptop default. |
| **"Forensic killer feature" doesn't land emotionally for new developers** — they don't yet have multi-agent pain. | The North Star moment misfires. | Three-agent example is paste-and-run; the pain is *manufactured* by the example (one of the three agents intentionally drops a heartbeat in a documented variant) so the trace command's value is visible without waiting for real production failure. |
| **Local UI becomes a generic chat/dashboard and dilutes the witness thesis.** | Product bloats and trust erodes. | v0.1 UI is constrained to observe, inspect chronology, send safely, and confirm outcome. Hosted dashboard, accounts, workflow editor, and full chat workspace are explicit non-goals. |
| **Category-creation thesis fails** ("just another local message bus"). | "SQLite of agent buses" never sticks. | The product still has standalone value as a local IPC fabric for agent prototypes; v0.5 re-positions toward platform-engineer secondary persona; v1.0 federation creates a clearer differentiation against pure single-machine peers. |
| **Innovation theater accusation** ("you're just bundling four off-the-shelf patterns"). | Credibility damage in technical communities. | Lean into it. Brief explicitly says "**No new ideas — just the right ones, packaged for the developer machine.**" The compound is the innovation; the constituent patterns are deliberately boring. The talk track for HN is "this is what should have existed already." |

## Developer-Tool / CLI / System-Daemon Specific Requirements

### Project-Type Overview

Zorn Mesh is **four project types in one binary**:

- **Developer tool** — distributed via package managers (`cargo`, `npm`, future `pip`), consumed as a Rust crate (`zorn-mesh-sdk`), TypeScript package (`@zornmesh/sdk`), and at v0.2 a Python package (`zorn-mesh-py`). Adopters write code against the SDK; the SDK is the API surface.
- **CLI tool** — `zornmesh` is a multi-command binary (`daemon`, `tail`, `trace`, `agents`, `inspect`, `doctor`, `replay`, `stdio`, `audit`). The CLI is both an operator surface (`daemon`, `doctor`) and a developer surface (`trace`, `tail`, `inspect`, `replay`).
- **Local web control plane** — `zornmesh ui` opens a browser-based companion UI served from bundled local assets. It is the visual surface for live agent status, trace chronology, safe sends, and daemon trust state; it is not a hosted dashboard.
- **System daemon (informally)** — `zornmesh daemon` owns a Unix-domain socket, a SQLite file, and a PID file; auto-spawns on first SDK connect; runs in user-space (never as root); has an explicit lifecycle and graceful-shutdown contract. **Not** an `init` system service at v0.1 (no `systemd` unit shipped); operators who want supervised mode write their own unit.

### Technical Architecture Considerations

**Cargo-workspace monorepo (locked, see project-context):**

- Eight crates with strict layering: `zornmesh-{proto, core, store, broker, rpc, daemon, cli, sdk}`.
- Layering rule: lower crates do not depend on higher crates; `proto` is the only crate every other depends on.
- `proto/zorn/mesh/v0/{envelope, handshake, service}.proto` is the canonical schema source.
- Non-Rust SDKs at `sdks/typescript` and `sdks/python` (v0.2).
- `release` profile: `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`. Single static binary ≤ 22 MB.

**Daemon lifecycle (auto-spawn invariant):**

- SDK calls `Mesh.connect()` → `try_connect → spawn_daemon_if_absent → retry_connect` (Tauri sidecar / sccache pattern).
- Cold-start budget: **≤ 200 ms** to "accepting connections" emitting `{"event":"ready","socket":"<path>"}` to stdout.
- Concurrent-connect race: 10 simultaneous SDK `connect()` calls produce **one** daemon, not ten (PID-file + lockfile correctness, CI gate).
- `ZORN_NO_AUTOSPAWN=1` opts out for production / CI runners.
- Daemon refuses to start as root with named error code; `zornmesh doctor` flags privileged execution.

**Storage (single-writer SQLite):**

- One file at `~/.local/state/zornmesh/mesh.db` (or `${XDG_STATE_HOME}/zornmesh/mesh.db`).
- WAL mode; writer connection pool `max_connections = 1`.
- Forward-only migrations at `/migrations/NNNN_*.sql`, applied at daemon startup; CI gate verifies every migration applies cleanly to a blank DB.
- Memory budget: ~96 MiB SQLite cache + ~64 MiB mapped DB region.

### Language Matrix

| SDK | v0.1 | v0.2 | Toolchain | Package |
|---|---|---|---|---|
| **Rust** | ✅ | (mature) | `cargo`, `tokio` 1.47 LTS, `tower`, `prost`, `rusqlite` 0.32, `tonic` 0.12 (fallback) | `cargo install zornmesh`; SDK crate `zorn-mesh-sdk` on crates.io |
| **TypeScript** | ✅ | (polish) | Bun-first first-party toolchain; `@bufbuild/protobuf` for codegen; Zod 4 at boundary; npm-compatible package output | `npm i @zornmesh/sdk` |
| **Python** | ❌ (v0.2) | ✅ | Python 3.11+ (mandatory for `asyncio.timeout()`); `betterproto` runtime + Pydantic v2; `uv` for envs | `pip install zorn-mesh-py` (or `uv add zorn-mesh-py`) |

**Locked language-specific rules** (from project-context, machine-enforced where marked):

- **Rust:** crate-root `#![deny(...)]` includes `unsafe_code` (gated), `missing_docs`, and rustc 1.78+ object-safety lints. Adding an `async fn` to a public trait without `#[async_trait]` or `dyn-compatible(false)` is a build break.
- **TypeScript:** `Symbol.dispose` is required on every type that holds a socket, file handle, or IPC connection (machine-enforced via custom ESLint rule on `@resource` JSDoc tag). Resource cleanup uses the `using` keyword, **never** raw `try/finally`.
- **Python:** every coroutine that touches I/O or external resources MUST be called under `asyncio.timeout()` (Python 3.11+). `timeout=None` is forbidden in library code; permitted only in CLI entrypoints where the user is the cancellation source. `Symbol.dispose` equivalent is `__aexit__` on async resource managers.
- **Cross-language:** `Secret<T>` wrapper redacts at `toJSON` / `toString` / `Symbol.toPrimitive` (TS), `Display` (Rust), `__repr__` (Python). Logging a raw secret is a lint failure in all three.

### Installation Methods

**v0.1 (Linux + macOS only):**

| Channel | Command | Artifact |
|---|---|---|
| Cargo | `cargo install zornmesh` | Builds `zornmesh` binary from source, installs to `~/.cargo/bin/` |
| npm | `npm i -g @zornmesh/cli` | Pre-built binary download, post-install hook places `zornmesh` in PATH |
| Direct download | `curl -fsSL https://zornmesh.dev/install.sh \| sh` | Sigstore-signed pre-built binary |
| Homebrew (macOS) | `brew install zornmesh/tap/zornmesh` | Tap-hosted bottle |

**v0.2:** Windows support via `scoop install zornmesh` and `winget install zornmesh`; named-pipe ACL story documented per-SDK.

**Verification:** every release artifact carries a Sigstore signature and a CycloneDX SBOM. `zornmesh doctor` displays the verified signature on the running binary at startup.

### API Surface (SDK — what adopters call)

The SDK is **not** a thin RPC client — it manages connection lifecycle, registration, capability advertisement, idempotency, retry, and trace propagation. Adopters call high-level methods.

**Core SDK methods (Rust + TS at parity in v0.1):**

```
Mesh.connect(options) → Mesh                  // auto-spawn + handshake
mesh.register(agent_card)                     // identity + capability advertisement
mesh.publish(subject, payload, options)       // fire-and-forget with backpressure
mesh.subscribe(pattern, handler, options)     // durable, with ACK/NACK on handler return
mesh.request(target, payload, options) → Response   // request/reply with correlation_id
mesh.fetch(subject, lease_options) → Pull     // pull-based delivery with leases
mesh.ack(message_id) / mesh.nack(message_id, reason)
mesh.stream(target, payload) → AsyncIterable<Chunk>  // 256 KiB byte-budget window
mesh.cancel(correlation_id)
mesh.disconnect()                             // graceful, drains pending ACKs
```

**Cross-cutting:** all methods accept an explicit `idempotency_key` (mandatory for at-least-once correctness); all methods propagate W3C tracecontext automatically; all resource-holding return values implement `Symbol.dispose` (TS), `Drop` (Rust), `__aexit__` (Python).

### CLI Surface (`zornmesh` subcommands)

| Subcommand | Purpose | Surface |
|---|---|---|
| `zornmesh daemon` | Start/stop/status the daemon explicitly | Operator + CI |
| `zornmesh stdio --as-agent <id>` | MCP-stdio bridge (launch-gate forcing function) | Adopter |
| `zornmesh trace <correlation_id>` | Reconstruct conversation timeline | Developer |
| `zornmesh tail [--subject <pattern>]` | Live-tail envelopes (like `tail -f`) | Developer |
| `zornmesh agents` | List registered agents and their capabilities | Developer + Operator |
| `zornmesh inspect <table> [--filter ...]` | Inspect SQLite tables with structured filters (`messages`, `dead_letters`, `audit_log`, `sbom`, `version`) | Developer + Operator |
| `zornmesh replay --correlation-id <id>` | Re-deliver an envelope from the audit log | Developer (recovery) |
| `zornmesh doctor` | Self-diagnostic: version, socket, schema, OTel reachability, trust posture | Operator |
| `zornmesh audit verify` | Offline tamper-evidence check on `audit_log` | Compliance |
| `zornmesh ui [--no-open] [--print-url] [--no-input]` | Open the local web control plane; print a protected loopback URL for scripted handoff | Developer + Operator |

### Local Web UI Surface (`zornmesh ui`)

The local web UI is a v0.1 product surface, not a future dashboard. It must preserve the same local-first trust model as CLI and SDK surfaces.

- **Launch behavior:** `zornmesh ui` starts or connects to the daemon, binds a loopback-only UI/API listener, creates a protected local session, opens the default browser unless `--no-open` is set, and can print the protected URL for scripts with `--print-url`.
- **Primary layout:** Live Mesh Control Room by default: agent roster/status, message/trace timeline, selected event detail, daemon/trust status, and safe composer.
- **Focused trace mode:** Focus Trace Reader shows one correlation ID as an ordered, complete, causally linked timeline with selected-event detail and delivery state.
- **Safe send behavior:** direct send targets one agent; broadcast requires recipient preview, explicit confirmation, capability warnings, and per-recipient outcomes.
- **Live updates:** UI ordering is based on daemon sequence, not browser receipt time. Reconnect performs backfill before declaring the view current.
- **Security posture:** loopback-only, session-token protected, CSRF/origin checked, no external assets, no remote config, no direct SQLite access.
- **CLI handoff:** every UI recovery action that has a stable CLI equivalent shows a copyable command.

### Output Formats

The CLI is **scriptable-first**; interactive use is sugar.

- **`--output json`** is supported on every read subcommand (`agents`, `inspect`, `tail`, `trace`); JSON output is the contract, not the human format.
- **Default human format** uses `tabwriter`-aligned columns; ANSI color is autoremoved when `stdout` is not a TTY (or `NO_COLOR` env var is set, per [no-color.org](https://no-color.org)).
- **Streaming subcommands** (`tail`) emit one JSON object per line (NDJSON / JSONL).
- **`--quiet`** suppresses non-essential output for use in scripts; `--verbose` adds debug context; `-v` / `-vv` / `-vvv` map to log levels.

### Configuration Schema

Configuration follows XDG and the principle "env var wins, file is fallback, daemon-flag wins all."

| Source | Path / Var | Purpose |
|---|---|---|
| Daemon flags | `zornmesh daemon --socket-path … --metrics-addr …` | Explicit override (highest precedence) |
| Env vars | `ZORN_SOCKET_PATH`, `ZORN_NO_AUTOSPAWN`, `ZORN_SHUTDOWN_BUDGET_MS`, `ZORN_METRICS_MAX_LABEL_VALUES`, `ZORN_CONFIG_DIR` | Per-process / per-CI override |
| Config file | `${ZORN_CONFIG_DIR}/zornmesh.toml` (default `${XDG_CONFIG_HOME}/zornmesh/`) | Persistent local config |
| Capability config | `${ZORN_CONFIG_DIR}/high-privilege-capabilities.toml` and `/etc/zorn-mesh/high-privilege-capabilities.toml` | Operator-managed capability gating |

**No remote config fetch**, ever. Local-first means local config.

### Shell Completion

- **Bash, Zsh, Fish:** `zornmesh completions <shell>` emits the completion script (clap-derived); shipped instructions in README.
- **Nu and PowerShell:** v0.2 (PowerShell ships alongside Windows support).
- Completions are dynamic for `--subject` (live-queries the daemon's known subject space when available).

### Scripting Support

- **Exit codes are stable:** `0` success; `1` user error (bad arguments); `2` daemon-unreachable; `3` validation failure; `4` permission denied; `5` not-found; `64–78` from `<sysexits.h>`. Documented in `man zornmesh` and `zornmesh --help`.
- **`--no-input`** flag asserts no interactive prompts will be shown; if any prompt would be needed, fail with exit code 1.
- **`xargs`-friendly:** `zornmesh agents --output json | jq -r '.[].id'` is a documented idiom; every subcommand emits stable JSON.

### Documentation

| Audience | Artifact | Where |
|---|---|---|
| Developers | API rustdoc, TypeScript TSDoc, Python pdoc | Generated from inline doc comments (machine-enforced via `cargo doc --no-deps -D missing_docs`, `tsdoc-required` ESLint rule, `pydocstyle` strict) |
| Adopters (first-run) | "Three-agent example in 10 minutes" tutorial | `examples/three_agents.{rs,ts}` + `README.md` |
| Operators | Operator guide (daemon lifecycle, retention config, SBOM, signing, `zornmesh doctor` reference) | `docs/operator/` (ships with binary as `zornmesh manual operator`) |
| Compliance reviewers | EU AI Act / NIST AI RMF / SOC 2 / GDPR mapping docs | `docs/compliance/` |
| Adapter authors | Symmetric capability model playbook + bridge author guide | `docs/adapter-authors/` |
| CLI users | `man zornmesh` and `zornmesh <cmd> --help` | Generated from clap, ships in binary |
| UI users | Local UI first-run, trace reader guide, safe direct/broadcast guide, local trust/security notes | `docs/ui/` and in-product help |

### Code Examples

- **Three-agent example** (`examples/three_agents.{rs,ts}`) is the canonical first-run experience. CI-tested every PR; **regression here is a hard launch-blocker.**
- **Streaming example** (`examples/streaming_chunks.{rs,ts}`) demonstrates the 256 KiB byte-budget window with a token-streaming use case.
- **MCP-stdio bridge example** (`examples/mcp_stdio_bridge.md`) walks Claude Desktop / Cursor users through `zornmesh stdio --as-agent <id>`.
- **Forensic recovery example** (`examples/forensic_recovery.md`) walks Daniel-persona's J2 journey end-to-end with a deliberately-broken three-agent variant.
- **Local UI example** (`examples/local_ui_trace.md`) walks Maya through `zornmesh ui`, live mesh visibility, and Focus Trace Reader.
- **Safe broadcast example** (`examples/safe_broadcast.md`) demonstrates recipient preview, explicit confirmation, per-recipient outcomes, and CLI handoff.
- **Adapter author example** (v0.5) — how to ship a Zorn Mesh adapter inside another agent runtime.

### Migration Guide

- **`buf breaking` CI gate** against the previous tag enforces wire stability — there is no breaking-migration story at v0.1.
- **SDK semver** drifts independently per language (`build_version` per SDK); a Rust security patch bumps `zorn-mesh-rust.build_version` without touching the TS or Python builds.
- **Wire deprecation policy** formalized at v1.0: minimum 6-month notice + dual-version daemon support before any field removal.
- **Pre-v1.0 wire changes** are documented in `CHANGELOG.md` with explicit migration steps; alpha consumers get a one-time courtesy notice for any SDK/runtime tooling change that affects installation or generated package output.

### Implementation Considerations

**Build / release:**

- Cross-compilation targets at v0.1: `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` (static), `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. Windows added at v0.2.
- **Reproducible builds** where toolchain permits; `SOURCE_DATE_EPOCH` respected.
- **Release artifacts:** binary, signature (Sigstore), SBOM (CycloneDX), checksum (`SHA256SUMS` + `.asc`).
- **Release cadence:** semver; patch releases as needed; minor releases roughly every 6 weeks post-v0.1.

**Testing discipline (locked, project-context):**

- `cargo nextest` + `proptest` + `loom` (concurrency) + `turmoil` (deterministic distributed sim).
- Shared daemon fixture crate `zorn-mesh-test-harness` exposes a launcher API consumed by all three SDK test suites — no SDK rolls its own daemon launcher.
- In-tree OTel collector at `/test-infra/otel-collector.yaml` launched by the harness; integration tests assert on span structure via this collector.
- Coverage gates: line ≥ 70% per package, branch ≥ 90% on routing-critical modules.
- Conformance fixtures under `/conformance/{security, observability, persistence, …}/` are first-class — every "MUST"/"MUST NOT" rule is anchored to a fixture.

**IDE integration:**

- **VS Code:** `rust-analyzer` extension is the primary developer experience; no Zorn-specific extension at v0.1.
- **JetBrains:** IntelliJ Rust plugin works; no special integration.
- **Editor-agnostic:** the CLI is the universal editor integration — `zornmesh trace`, `zornmesh tail`, `zornmesh inspect` work in any terminal.
- **Cursor / Claude Desktop / Copilot:** integration is **via the MCP-stdio bridge**, not via custom extensions. This is deliberate — adapter cost stays at zero for host vendors.

**Sections deliberately constrained** (per product scope, not skipped):

- Visual design / UX principles apply to the local web UI and are documented in the UX specification; implementation must preserve calm dark-first developer-console defaults, light mode, keyboard/focus visibility, and reduced motion.
- Touch interactions are fallback-only: tablet/mobile layouts must remain functional, but desktop/laptop is the primary v0.1 target.
- Store compliance (App Store, Play Store, Microsoft Store) is out of scope because distribution is via package managers and signed release artifacts, not app stores.
- Mobile-native features and Web SEO are out of scope because the UI is a local developer-machine tool, not a public website.

## Project Scoping & Phased Development

### MVP Strategy & Philosophy

**Approach: Problem-solving MVP with a forcing function.**

The launch gate "MCP-stdio bridge works with a real host" is the forcing function — it dogfoods the entire wire (handshake, capability advertisement, request/reply, streaming, observability, persistence) end-to-end against a host the project does not control. If any layer is broken, the bridge breaks visibly. A coordination broker that cannot be observed coordinating with a real adopter is unfalsifiable; this gate makes it falsifiable.

**Resource model:** solo-developer-feasible at v0.1 by architectural design (Cargo workspace, CI gates, conformance fixtures, in-tree OTel collector, deterministic test harness). No team-size assumption is encoded; the architecture is what makes it solo-feasible.

**Party-mode dissents recorded** (raised during this scoping round; deliberately not adopted at v0.1):

- *John (PM):* "11 must-haves → 6; cut SOC 2 / EU AI Act / durable subs to v0.3; reduce to one persona (Maya)." Held: compliance baseline is a credibility prerequisite for the Sam-track due-diligence journey at v0.1, even if Sam isn't the v0.1 buyer.
- *Mary (Analyst):* "Move Sam into v0.1 and reframe launch gate around LangGraph/Mastra adapter; promote Python SDK to v0.1; strip compliance as theater." Held: adapter SDK is deliberately a v0.5 milestone (forces v0.1 to ship the symmetric capability primitives that adapters will consume); MCP-stdio bridge is the more falsifiable launch gate against a host the project does not control.
- *Winston (Architect):* "Demote auto-spawn to v0.2 in favor of explicit `zornmesh daemon` + socket health check." Held: auto-spawn is a sold innovation pattern (sccache transfer) and the developer-experience contract — `Mesh.connect()` Just Works. The PID-file + lockfile + 10-concurrent-connect CI gate is the mitigation. `ZORN_NO_AUTOSPAWN=1` is the explicit-mode escape hatch.
- *Amelia (Dev):* "Many v0.1 must-haves lack measurable ACs; missing implicit requirements (max envelope size, backpressure semantics, idempotency window, dead-letter triggers, SIGTERM drain timeout, SQLite disk-full behavior, Sigstore enforcement scope)." **Adopted by reference into Step 9** — these gaps are functional-requirement work, not scoping work, and will be enumerated with ACs in Step 9.

These dissents are preserved here so that future readers can see the tradeoffs that were considered and the rationale for the decisions taken.

### MVP Feature Set (Phase 1 — v0.1, Launch)

**Launch gate (hard, machine-enforced where possible):**

1. MCP-stdio bridge connects Claude Desktop **or** Cursor and exchanges ≥ 100 envelopes without protocol error.
2. Three-agent example runs to completion in ≤ 10 minutes from `cargo install`.
3. `zornmesh doctor` reports green on a clean install across the v0.1 platform matrix.
4. `zornmesh ui` opens a protected loopback local web UI with three live agents visible within 2 seconds of daemon readiness.
5. Browser E2E completes observe → inspect trace → direct send → broadcast preview/confirmation → per-recipient outcomes.

**Core User Journeys Supported:**

- **J1 — Maya** (Cursor primary success): primary v0.1 journey; the smoke-test that all wire primitives work end-to-end inside a host vendor's IDE.
- **J2 — Daniel** (forensic 2 a.m. recovery): `zornmesh trace`, `zornmesh replay`, audit-log durability — proves persistence is real, not aspirational.
- **J3 — Priya** (Claude Desktop stdio bridge): proves the MCP-superset wire degrades cleanly to MCP-stdio for hosts that can't speak the full wire.
- **J4 — Sam** (platform engineer due-diligence, evaluation-only at v0.1): SBOM + Sigstore + compliance mapping must be present so the v0.1 build survives a Sam-grade gate review even if no Sam adopts at v0.1.
- **J5 — Safe direct/broadcast intervention:** proves humans can send through the same auditable broker semantics without creating an invisible side channel.

**Must-Have Capabilities:**

1. **Wire & broker.** MCP-superset envelope (handshake, publish, subscribe, request/reply, fetch/lease, streaming with 256 KiB byte-budget, ack/nack, cancel); subject hierarchies; durable subscriptions; backpressure for consumers that reach queue or acknowledgement bounds.
2. **SDKs.** Rust SDK (`zorn-mesh-sdk`) and TypeScript SDK (`@zornmesh/sdk`) at parity for all wire methods; parity defined by a shared conformance fixture per method.
3. **Daemon lifecycle.** Auto-spawn (Tauri / sccache pattern), ≤ 200 ms cold start to "accepting connections," single-daemon-per-machine PID-file invariant (10-concurrent-connect CI gate), graceful drain on SIGTERM bounded by `ZORN_SHUTDOWN_BUDGET_MS`, `ZORN_NO_AUTOSPAWN=1` opt-out.
4. **Persistence.** Single-writer SQLite, WAL mode, forward-only migrations applied at daemon startup, audit log, dead-letter queue, table-specific configurable retention (defaults: messages 24 h, DLQ 7 d, audit log 30 d — see Domain Requirements §Privacy and data handling).
5. **Identity & capabilities.** Agent cards, symmetric capability model, `high-privilege-capabilities.toml` operator gate.
6. **CLI.** `zornmesh {daemon, stdio, trace, tail, agents, inspect, doctor, replay, audit verify}` with stable exit codes and `--output json` on every read subcommand.
7. **Observability.** W3C tracecontext propagation, OTel metrics + traces, in-tree OTel collector for tests.
8. **Security.** Unix-domain socket mode `0600`, refuse-as-root with named error code, `Secret<T>` redaction across Rust + TS (and Python at v0.2), Sigstore-signed releases, CycloneDX SBOM in every release.
9. **Compliance baseline.** EU AI Act traceability fields on every envelope, NIST AI RMF alignment doc, GDPR DPIA template, SOC 2 evidence-export script. Documentation-grade for v0.1 — sufficient for Sam due-diligence, not pre-audit.
10. **MCP-stdio bridge.** `zornmesh stdio --as-agent <id>` with documented Claude Desktop and Cursor configurations.
11. **Local web companion UI.** `zornmesh ui` with Live Mesh Control Room, Focus Trace Reader, safe direct/broadcast composer, per-recipient outcome list, daemon/trust status, CLI handoff copy blocks, and browser E2E fixture coverage.
12. **Distribution.** `cargo install`, `npm i -g @zornmesh/cli`, `install.sh` for Linux + macOS (x86_64 + aarch64), with bundled local UI assets included in release artifacts.

**Out of scope at v0.1 (deliberate non-goals, traceable to brief and distillate):**

- Python SDK (v0.2) — Rust + TS demonstrate the cross-language model; Python expands reach.
- Windows support (v0.2) — named-pipe + ACL story is non-trivial and not on the v0.1 critical path.
- Adapter SDK for embedding inside other agent runtimes (v0.5).
- Encryption-at-rest for SQLite (operator opt-in via filesystem; not built-in).
- Multi-host federation (v1.0+; explicitly may never ship — local-first is the thesis).
- Hosted/cloud dashboard, LAN/public web console, accounts/teams, full chat workspace, workflow editor, and remote collaboration features.
- `systemd` unit / launchd plist (operators write their own).
- Remote config fetch (never, by design).

### Post-MVP Features

**Phase 2 — Growth v0.2 (theme: language reach + platform reach):**

- Python SDK (`zorn-mesh-py`) at parity with Rust + TS.
- Windows support: named-pipe transport, per-SDK ACL story, `scoop` + `winget` distribution.
- Shell completions for PowerShell.
- Operator-guide hardening informed by v0.1 field reports.
- `mesh-trace/1.0` draft published as a candidate standard.

**Phase 3 — Adapter v0.5 (theme: zero-cost embedding):**

- Adapter SDK so other agent runtimes (LangGraph, Mastra, custom orchestrators) ship a Zorn Mesh adapter via one import.
- Adapter-author guide.
- Adapter-conformance test suite.
- Advanced UI expansion: richer topology analysis, saved views, expanded DLQ/replay workflows, and cross-session search.

**Phase 4 — Vision v1.0+ (theme: standards & long-tail):**

- Wire stability commitment with formal deprecation policy (≥ 6-month notice + dual-version daemon support before any field removal).
- `mesh-trace/1.0` standardization.
- Optional federation — only if field demand demonstrates local-first hits a real ceiling; otherwise explicitly rejected.
- Compliance certifications (SOC 2 Type II audit by a customer; FedRAMP Moderate path if a federal adopter funds it).

### Risk Mitigation Strategy

| Risk class | Specific risk | v0.1 mitigation |
|---|---|---|
| **Technical — concurrency** | Auto-spawn race producing multiple daemons on concurrent SDK connect | PID-file + lockfile + 10-concurrent-connect CI gate |
| **Technical — performance** | ≥ 5,000 env/sec target unmet | `criterion` benchmarks gated in CI; `loom` model-checks the routing core; baseline measured before v0.1 wire-freeze |
| **Technical — wire instability** | Breaking change shipped accidentally | `buf breaking` CI gate against the previous tag |
| **Technical — bootstrap deadlock** | Auto-spawn fails on a hardened machine; every SDK call hangs | `ZORN_NO_AUTOSPAWN=1` opt-out; `zornmesh doctor` diagnoses; mandatory explicit timeout on `connect()` (Python via 3.11+ `asyncio.timeout()`) |
| **Technical — daemon-lifecycle footguns** | Stale PID, permission races on restart, signal-handling edges | Socket-based health check supplements PID-file; `zornmesh doctor` exercises restart paths in CI |
| **Market — adoption** | "Why not just use NATS / Redis Streams / raw MCP?" | Compound positioning (Step 6); MCP-stdio bridge demo as the unfalsifiable proof |
| **Market — host platform shift** | MCP wire evolves and breaks the bridge | Conformance suite tracks the MCP spec; superset design is *more* permissive than MCP, never less |
| **Market — Python ecosystem reach** | LangGraph / Mastra / AutoGen are Python-first; deferring Python to v0.2 caps v0.1 addressable market | Accepted as a v0.1 cost; Python parity is the v0.2 headline; adapter SDK at v0.5 retroactively absorbs runtime-embedded use cases |
| **Resource — solo developer** | Burnout / bus factor on a single maintainer | Architecture is solo-feasible by design; CI gates substitute for review headcount; documentation is shipped as code (machine-enforced doc lints) |
| **Resource — community contribution gap** | Adapter SDK won't ship if no third-party adopters | Adapter SDK is v0.5, not v0.1 — explicitly delayed until v0.1 demonstrates demand |
| **Compliance — EU AI Act applicability** | Adopters in regulated sectors block deployment | Traceability fields ship at v0.1; compliance mapping doc ships with binary; audit-log tamper-evidence verified by `zornmesh audit verify` |
| **Compliance — secrets leakage** | Audit log contains payloads; payloads contain secrets | `Secret<T>` wrapper across Rust + TS (Python v0.2); redaction-lint failure in CI; `zornmesh inspect` filters known-secret patterns |
| **UX — local UI stale or confusing** | Developer misdiagnoses a trace because browser state lags daemon truth | Daemon-sequence ordering, reconnect/backfill fixture, explicit stale/disconnected UI states, and CLI trace as reference surface |
| **Security — UI exposure** | Local control plane reachable beyond intended browser/session | Loopback-only bind, session token, CSRF/origin checks, no external assets, and refusal to bind public interfaces at v0.1 |

## Functional Requirements

### Wire & Messaging

- **FR1:** An Agent can publish an envelope to a subject with at-least-once delivery semantics.
- **FR2:** An Agent can subscribe to a subject pattern (exact, prefix, or hierarchical wildcard) and receive matching envelopes.
- **FR3:** An Agent can issue a request to a target agent and receive exactly one correlated reply, or a typed timeout / cancellation result.
- **FR4:** An Agent can fetch envelopes via pull-based delivery with explicit lease acquisition and renewal.
- **FR5:** An Agent can stream a response to a request as a sequence of chunks bounded by an explicit byte-budget window.
- **FR6:** An Agent can acknowledge or negative-acknowledge a delivered envelope, with a structured reason on negative acknowledgement.
- **FR7:** An Agent can cancel an in-flight request or stream by correlation ID.
- **FR8:** An Agent can supply an idempotency key on any send-side operation, and the broker enforces at-least-once-with-deduplication semantics within a defined uniqueness window.
- **FR9:** An Agent can establish a durable subscription that survives daemon restart and resumes from the last acknowledged envelope.
- **FR10:** The broker can apply backpressure when a consumer reaches its configured queue bound or misses its acknowledgement/lease budget, with publisher-visible feedback and no silent drops unless explicitly configured.

### Identity & Capabilities

- **FR11:** An Agent can register an agent card declaring identity, version, and the set of capabilities it advertises and consumes.
- **FR12:** An Agent can advertise capabilities symmetrically — the same capability primitives govern what an agent offers and what it consumes.
- **FR13:** An Operator can mark a capability as high-privilege, restricting which agents may advertise or invoke it.
- **FR14:** An Adopter can resolve the current set of registered agents and their advertised capabilities.

### Daemon Lifecycle

- **FR15:** An Adopter can connect to the mesh from an SDK and have the daemon auto-spawn if not already running, without intervention.
- **FR16:** An Operator can disable auto-spawn and require explicit daemon startup via an environment variable.
- **FR17:** An Operator can start, stop, and query daemon status explicitly from the CLI.
- **FR18:** The daemon enforces that exactly one instance owns the broker socket per machine, even under concurrent connection attempts.
- **FR19:** The daemon refuses to run with elevated privileges (root / Administrator) and reports a named error.
- **FR20:** The daemon performs a graceful drain on shutdown, bounded by an operator-configurable budget.
- **FR21:** An Operator can run a self-diagnostic that reports daemon version, socket path, schema version, OTel reachability, signature verification, and trust posture.

### Persistence & Forensics

- **FR22:** The broker persists every envelope to a tamper-evident audit log durable across daemon and host restart.
- **FR23:** The broker captures undeliverable envelopes or envelopes whose retry budget or TTL is exhausted into a dead-letter queue with structured failure reason.
- **FR24:** A Developer can reconstruct the timeline of a conversation by correlation ID across all participating agents.
- **FR25:** A Developer can re-deliver a previously-sent envelope from the audit log to support recovery scenarios.
- **FR26:** A Developer can inspect persistence state (messages, dead letters, audit log, SBOM, schema version) with structured filters.
- **FR27:** An Operator can configure retention policy by age, count, and capability class, with defaults of 24 h messages, 7 d DLQ, and 30 d audit log.
- **FR28:** A Compliance Reviewer can verify audit-log tamper-evidence offline without daemon access.

### Observability & Tracing

- **FR29:** The broker propagates W3C tracecontext across every wire operation without adopter intervention.
- **FR30:** The broker emits OpenTelemetry metrics and traces conforming to a documented schema.
- **FR31:** A Developer can live-tail envelopes by subject pattern as structured records.
- **FR32:** A Developer can reconstruct a span tree for a request/reply or streaming exchange end-to-end.

### Host Integration

- **FR33:** A Host (e.g., Claude Desktop, Cursor) can connect to the mesh via an MCP-stdio bridge that exposes a registered agent.
- **FR34:** An Adopter can register the same agent across multiple host connections and have the broker route to a single canonical identity.
- **FR35:** When the connected host speaks only baseline MCP, the bridge exposes only mesh capabilities representable on the MCP wire and returns a named unsupported-capability result for the rest.

### Security & Trust

- **FR36:** An Adopter can mark fields as secret and the secret value is redacted in all log, metric, trace, and audit-log emissions.
- **FR37:** An Operator can verify that the running binary's signature matches the published Sigstore signature.
- **FR38:** An Operator can retrieve the SBOM (CycloneDX) of the running binary at runtime.
- **FR39:** An Adopter can never invoke a high-privilege capability without a corresponding operator-approved entry in the capability gate.
- **FR40:** The broker rejects connections that do not satisfy the local socket permission model.

### Compliance & Audit

- **FR41:** Every envelope carries the traceability fields required by the PRD's EU AI Act mapping evidence bundle: agent identity, capability invoked, timestamp, correlation ID, trace ID, and prior-message lineage.
- **FR42:** A Compliance Reviewer can export an evidence bundle (audit log slice, SBOM, signature, configuration snapshot) for a specified time window.
- **FR43:** A Subject Data Owner can request, via a documented procedure, the deletion of envelopes referencing their personal data, scoped to retention-policy obligations.
- **FR44:** A Compliance Reviewer can map any envelope to a documented NIST AI RMF function/category from a versioned mapping table, with unmapped envelopes reported explicitly and manual overrides audit-logged.

### Developer & Operator CLI

- **FR45:** A Developer or Operator can request structured (JSON) output on every read subcommand.
- **FR46:** A Developer or Operator can run any subcommand non-interactively under a flag that fails fast rather than prompting.
- **FR47:** A Developer can install shell completions for their shell from the CLI itself.
- **FR48:** Every CLI subcommand exits with a stable, documented exit code distinguishing user error, daemon-unreachable, validation failure, permission denied, and not-found.

### Local Web Companion UI

- **FR49:** A Developer can launch the local web UI from the CLI and receive either an opened browser window or a protected loopback URL suitable for copy/paste.
- **FR50:** A Developer can view connected agents live, including identity, status, capabilities summary, transport/source, warnings, and last-message recency.
- **FR51:** A Developer can view a message/trace timeline ordered by daemon sequence, with browser receipt time visible only as secondary diagnostic metadata.
- **FR52:** A Developer can inspect a selected trace event and see payload summary, causality, delivery state, timing, source/target agent, and associated CLI command where available.
- **FR53:** A Developer can open a focused trace view for one correlation ID and understand the full multi-agent conversation without stitching logs by hand.
- **FR54:** A Developer can send a direct message to one selected agent from the UI after reviewing target identity, capability summary, and payload preview.
- **FR55:** A Developer can broadcast a message only after reviewing the recipient list, excluded/incompatible recipients, payload summary, and explicit confirmation.
- **FR56:** A Developer can see per-recipient delivery outcomes for direct and broadcast sends, including queued, delivered, acknowledged, rejected, timed out, and dead-lettered states.
- **FR57:** A Developer can refresh or reconnect the UI and recover the selected trace context after daemon backfill completes.
- **FR58:** An Operator can see daemon/local trust state in the UI, including loopback-only status, session protection, socket path, schema version, and any stale/disconnected warning.
- **FR59:** The UI can display copyable CLI handoff commands for trace, inspect, replay, agents, doctor, and audit operations represented in the current context.
- **FR60:** Human-originated UI sends create auditable records linked to actor/session, target recipients, trace/correlation ID, payload summary, and delivery outcome.

### Adopter Extensibility

- **FR61:** An Adopter can build an agent in Rust or TypeScript at v0.1 (Python at v0.2), against an SDK that provides identical wire semantics across languages.
- **FR62:** An Adopter can carry a per-call idempotency key, trace context, and timeout through every SDK method without manual plumbing.

### Capability Contract Reminder

This FR list is binding. Any feature not enumerated above will not exist in the final product unless explicitly added through change control. Features deliberately not given FRs at v0.1 (per Step 8 scoping):

- Adapter SDK / agent-runtime embedding (v0.5).
- Multi-host federation (v1.0+; may never ship).
- Hosted/cloud dashboard, LAN/public web console, accounts/teams, workflow editor, and full chat workspace.
- Built-in encryption-at-rest.
- Remote config fetch (never, by design).
- Python SDK FRs (deferred to v0.2 — FR61 explicitly says "Rust or TypeScript at v0.1").

## Non-Functional Requirements

Selective NFR set: only categories that materially apply to a single-binary local broker and local web control plane for AI coding agents. Accessibility, browser compatibility, local UI security, and live-update consistency are v0.1 requirements because the local web UI is a first-class product surface.

### Performance

- **NFR-P1 — Cold start.** From SDK connect invocation on a clean machine to daemon readiness for local mesh traffic ≤ **200 ms** at the p95, measured on the v0.1 platform matrix on a reference dev laptop (8-core x86_64, NVMe).
- **NFR-P2 — Throughput.** Sustained ≥ **5,000 envelopes/sec** routed end-to-end (publisher SDK → broker → durable subscriber SDK with ack), three-agent topology, payload ≤ 4 KiB, persistence enabled. CI benchmark gate; regression > 10% fails the build.
- **NFR-P3 — Request/reply latency.** p50 ≤ **2 ms**, p99 ≤ **20 ms** for in-process loopback request/reply with ≤ 4 KiB payload, persistence enabled.
- **NFR-P4 — Streaming throughput.** Single stream ≥ **50 MiB/sec** sustained on the reference machine with the configured 256 KiB byte-budget window.
- **NFR-P5 — Memory ceiling.** Daemon resident memory ≤ **256 MiB** under nominal load (≤ 50 connected agents, ≤ 5,000 env/sec, default retention).
- **NFR-P6 — Backpressure responsiveness.** A blocked subscriber MUST surface backpressure to publishers within **100 ms**; the broker MUST NOT silently buffer beyond an operator-configurable per-subscription bound.
- **NFR-P7 — UI visible-mesh latency.** After daemon readiness, a local UI session MUST render currently connected agents within **2 seconds** for the 3-agent fixture.
- **NFR-P8 — UI trace render latency.** Opening a trace with ≤ 500 events MUST render the ordered timeline and selected-event detail within **1 second** on the reference machine.
- **NFR-P9 — UI send outcome latency.** Direct or 3-recipient broadcast sends MUST display terminal per-recipient outcomes within **5 seconds** unless an agent timeout policy explicitly exceeds that budget.

### Security

- **NFR-S1 — Local-first surface.** The agent data plane listens only on a Unix-domain socket (named pipe on Windows v0.2). The local web UI/API listener exists only when `zornmesh ui` is explicitly launched and MUST bind to loopback only.
- **NFR-S2 — Socket permissions.** Socket file mode `0600`, owned by the invoking user. Permissions are verified at `connect()` and a deviation aborts the connection with a named error.
- **NFR-S3 — Privilege refusal.** Daemon refuses to start as `uid 0` (Linux/macOS); reports `E_PRIVILEGED_REFUSED` with an explanatory message. Same posture for `Administrator` on Windows v0.2.
- **NFR-S4 — Secret redaction.** Any value wrapped in `Secret<T>` (Rust), tagged `@secret` (TS), or annotated `Secret[...]` (Python v0.2) MUST NOT appear in: stdout, stderr, log sinks, OTel metric attributes, OTel span attributes, audit-log payload fields, dead-letter records, or `zornmesh inspect` output. Lint gate fails CI on any code path emitting an un-redacted secret.
- **NFR-S5 — Supply-chain integrity.** Every release artifact (binary, SDK package) carries a Sigstore signature and a CycloneDX SBOM. `zornmesh doctor` displays both at startup and on demand. `cargo install` from source builds an SBOM at install time.
- **NFR-S6 — Capability gating.** Any capability marked high-privilege in `high-privilege-capabilities.toml` MUST be rejected at register-time and at invoke-time if the requesting agent is not allow-listed. Default policy is deny.
- **NFR-S7 — Wire validation.** Every inbound envelope is validated against the canonical envelope schema before any further processing; malformed envelopes are routed to a structured-error sink, never to subscribers.
- **NFR-S8 — No outbound network.** The daemon performs no outbound network call at runtime — no telemetry, no auto-update, no remote config. The only outbound socket is the OTel exporter, which is **off by default** and routes only to an operator-configured endpoint.
- **NFR-S9 — UI session protection.** The local UI requires a per-launch session token; state-changing requests require CSRF protection and origin checks. Missing or invalid protection fails closed.
- **NFR-S10 — No public UI bind.** v0.1 UI refuses `0.0.0.0`, non-loopback, LAN, or public hostname binding with a named error and remediation text.
- **NFR-S11 — Bundled assets only.** Local UI loads no CDN scripts, remote fonts, analytics, images, or remote config at runtime. Browser E2E fails on any external request except operator-configured OTel endpoints outside the browser.
- **NFR-S12 — UI redaction parity.** Browser-visible payload summaries follow the same `Secret<T>`/redaction rules as CLI, logs, traces, and audit output; raw secret display is a test failure.

### Reliability

- **NFR-R1 — Single-daemon invariant.** Under 10 concurrent SDK connect calls from cold, exactly one daemon process exists 95% of the time across 1,000 CI iterations and 100% of the time across 10 manual runs; enforcement must be race-safe and visible to diagnostics.
- **NFR-R2 — Persistence durability.** Every envelope acknowledged to a publisher MUST be recoverable by `zornmesh replay --correlation-id <id>` after a SIGKILL of the daemon followed by a clean restart. Verified by a dedicated crash-recovery fixture.
- **NFR-R3 — WAL crash recovery.** SQLite WAL recovery completes ≤ **2 seconds** for a database holding 7 days of default-retention audit log on the reference machine.
- **NFR-R4 — Graceful drain.** On SIGTERM, the daemon stops accepting new connections, drains in-flight requests bounded by `ZORN_SHUTDOWN_BUDGET_MS` (default 10 s, max 60 s), and exits 0; if the budget elapses, in-flight envelopes are written to dead-letter and the daemon exits with a code documenting partial drain.
- **NFR-R5 — Disk-full posture.** When SQLite write fails for `SQLITE_FULL`, the daemon enters a **read-degraded mode**: subscribes and reads continue; new publishes return `E_PERSIST_FULL` synchronously to the publisher; `zornmesh doctor` reports the condition. Daemon does not crash.
- **NFR-R6 — Migration safety.** Forward-only schema migrations apply atomically at startup; any migration failure leaves the database in its pre-migration state and the daemon refuses to start with a structured error pointing at the failed migration.
- **NFR-R7 — UI reconnect/backfill.** After browser disconnect, daemon restart, or tab refresh, the UI MUST backfill missed events by daemon sequence before marking the view current; stale/disconnected state must be explicit.

### Scalability

- **NFR-SC1 — Local-first ceiling (acknowledged).** Zorn Mesh targets **single-machine** deployment. Capacity beyond **200 connected agents per daemon** or **50,000 env/sec** is non-goal at v0.1; operators hitting these limits are explicitly directed to NATS or a federated topology in v1.0+.
- **NFR-SC2 — Subscription cardinality.** Default per-subject subscriber cap **256**; per-machine total subscription cap **4,096**; both operator-configurable. Exceeding caps is a typed error at `subscribe()`, never silent.
- **NFR-SC3 — Subject hierarchy.** Subjects support up to **8 hierarchical levels** and ≤ **256 bytes** total length; pattern matching scales to 4,096 active subscriptions with sub-millisecond match cost (CI benchmark gate).
- **NFR-SC4 — Envelope size.** Single envelope payload ≤ **8 MiB**; streaming chunks ≤ **256 KiB**; broker rejects oversize at framing with `E_ENVELOPE_TOO_LARGE`.
- **NFR-SC5 — Retention scale.** Defaults are table-specific (messages 24 h, DLQ 7 d, audit log 30 d). At 5,000 env/sec average with default settings, on-disk footprint stays within **12 GiB** combined across tables on the reference machine; all three retentions are independently operator-configurable. Retention sweep runs incrementally without blocking publish.
- **NFR-SC6 — Metric cardinality.** OTel label values per metric capped at **`ZORN_METRICS_MAX_LABEL_VALUES`** (default 10,000, per project-context §Observability); high-cardinality dimensions (correlation IDs, subject names, idempotency keys) are emitted as exemplars on traces, never as metric labels.

### Compatibility & Portability

- **NFR-C1 — v0.1 platform matrix.** Linux (glibc ≥ 2.31 and musl) and macOS (≥ 13 Ventura) on x86_64 and aarch64. Each combination ships a release artifact, signed and SBOM-attached.
- **NFR-C2 — SDK language matrix.** Rust and TypeScript SDKs ship at v0.1 with documented lower-bound runtime/toolchain support; Python SDK ships at v0.2 with documented lower-bound runtime support. CI matrix exercises each lower bound.
- **NFR-C3 — Wire stability discipline.** Wire compatibility checks against the previous tag reject any breaking change; pre-v1.0 breaking changes require an explicit `BREAKING:` commit and a CHANGELOG migration note. v1.0+ breaking changes require ≥ 6-month notice and dual-version daemon support.
- **NFR-C4 — MCP host conformance.** MCP-stdio bridge passes the MCP host conformance fixture for Claude Desktop and Cursor on the v0.1 platform matrix; conformance is re-run on every PR touching the bridge.
- **NFR-C5 — `NO_COLOR` & TTY.** CLI honors `NO_COLOR=1` and auto-disables ANSI when stdout is not a TTY. JSON output is byte-identical regardless of TTY.
- **NFR-C6 — Browser support.** Local UI targets current stable Chromium, Firefox, and Safari/WebKit on the v0.1 desktop/laptop platform matrix; tablet/mobile layouts are functional fallbacks, not primary targets.
- **NFR-C7 — Offline local assets.** The UI remains usable with network disabled after the local loopback session is established; all application assets ship with the release artifact.

### Observability

- **NFR-O1 — Tracecontext propagation.** Every wire operation propagates W3C `traceparent` and `tracestate` headers without adopter intervention; conformance fixture asserts the daemon emits one span per envelope per hop with parent-child linkage.
- **NFR-O2 — OTel schema.** Metrics and spans conform to a documented schema in `docs/observability/schema.md`; schema changes follow semver (additive = minor; rename/remove = major).
- **NFR-O3 — Self-instrumentation overhead.** Tracing on adds ≤ **5%** to NFR-P2 throughput at default sampling (1.0); tracing off adds ≤ **0.5%**.
- **NFR-O4 — Diagnostics latency.** `zornmesh doctor` returns within **1 second** on a healthy daemon; failure modes return within 3 seconds with a structured cause.

### Maintainability & Supportability

- **NFR-M1 — Reproducible builds.** Release builds respect `SOURCE_DATE_EPOCH` where the toolchain permits; identical inputs produce byte-identical binaries on the reference build host.
- **NFR-M2 — Test coverage gates.** Line coverage ≥ **70%** per crate / package; branch coverage ≥ **90%** on routing-critical modules (`zornmesh-core`, `zornmesh-broker`, `zornmesh-rpc`); CI enforces.
- **NFR-M3 — Conformance fixtures.** Every "MUST" / "MUST NOT" rule from this PRD is anchored to at least one fixture under `/conformance/`; CI verifies fixture coverage by tag.
- **NFR-M4 — Deterministic testing.** Deterministic concurrency, distributed-scenario, and property-based tests cover the routing core and wire-codec round-trips. Flaky tests are quarantined with an explicit issue link, never silenced.
- **NFR-M5 — Documentation freshness.** Public API documentation is machine-enforced for every SDK; a missing public doc string fails CI.
- **NFR-M6 — SDK independence.** Rust security patch can ship without bumping TS or Python SDK build versions; release pipeline supports per-SDK independent semver.
- **NFR-M7 — UI fixture coverage.** Browser E2E fixtures cover first open, live roster, trace inspection, safe direct send, safe broadcast, reconnect/backfill, keyboard-only navigation, and external-request blocking.

### Compliance & Auditability

- **NFR-CA1 — Audit-log tamper-evidence.** `audit_log` rows are chained by per-row hash; `zornmesh audit verify` detects any single-row tamper with probability ≥ 1 − 2⁻⁵⁰ and runs offline (no daemon required).
- **NFR-CA2 — Retention enforcement.** Configured retention deletes happen within **24 hours** of the threshold; deletions are themselves audited as `RETENTION_PURGE` records with capability-class breakdowns.
- **NFR-CA3 — Evidence export.** `zornmesh audit export --since … --until …` emits a single self-contained bundle (audit-log slice, SBOM, signature, sanitized config snapshot) within **5 minutes** for a 7-day window on the reference machine.
- **NFR-CA4 — Personal-data handling.** GDPR-style deletion requests are satisfied by `zornmesh audit redact --subject <id>` which replaces personal-data fields with redaction markers while preserving correlation IDs and audit-log integrity (the chain is rewritten with a `REDACTION_APPLIED` proof record).
- **NFR-CA5 — SBOM completeness.** Every direct and transitive dependency appears in the CycloneDX SBOM; CI fails if SBOM generation reports unaccounted dependencies.

### Accessibility

- **NFR-A11Y1 — WCAG AA baseline.** Core UI flows meet WCAG AA contrast and text legibility requirements in dark-first mode and light mode.
- **NFR-A11Y2 — Keyboard access.** Agent roster, trace timeline, detail panel, safe composer, broadcast confirmation, and command-copy controls are fully operable by keyboard with visible focus.
- **NFR-A11Y3 — Screen reader semantics.** Trace timelines expose ordered structure, selected state, delivery state, and causality labels without relying on visual position alone.
- **NFR-A11Y4 — Reduced motion.** Live-update animations respect `prefers-reduced-motion`; essential state changes remain perceivable without animation.
- **NFR-A11Y5 — No color-only status.** Agent status, delivery outcome, stale/disconnected state, warnings, and errors use text/icon/shape in addition to color.
- **NFR-A11Y6 — CLI accessibility remains intact.** CLI surfaces continue to honor `NO_COLOR` and stable JSON output so automation and assistive tooling are not forced through the UI.
