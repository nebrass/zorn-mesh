---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary', 'step-03-success']
inputDocuments:
  - "_bmad-output/project-context.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh-distillate.md"
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
  projectTypeAddenda: ['cli_tool', 'system_daemon_informal']
  domain: 'developer_infra'  # informal — supersedes BMad CSV's 'general'; complexity override required
  domainCsvFallback: 'general'  # for any tooling that reads CSV taxonomy
  complexity: 'high'
  projectContext: 'greenfield'
  primaryJob: 'observability_of_broken_multi_agent_system'  # "know exactly what happened, and when"
  primaryPersona: 'individual_developer'  # buyer = user at v0.1
  futurePersona: 'platform_engineer'  # buyer ≠ user, flagged so v0.1 decisions don't quietly foreclose
  launchGate: 'mcp_stdio_bridge_with_major_host'  # forcing function — Claude Desktop or Cursor at v0.1
  mandatoryAddenda:
    - 'operations_and_lifecycle_section'  # daemon state machine, socket ownership, SQLite contract, upgrade protocol, crash recovery, zornmesh doctor spec
    - 'compliance_and_regulatory_section'  # EU AI Act, NIST AI RMF, NIST SP 800-218A, GDPR, SOC 2, CISA SBOM
    - 'stakeholder_map_section'  # enterprise security reviewers, agent-runtime vendors, protocol stewards, OSS contributors, enterprise buyers, regulatory watch
  classificationProvenance: 'party-mode review by Mary (BA), John (PM), Winston (Architect) — 8 enrichments accepted by user'
---

# Product Requirements Document — zorn-mesh

**Author:** Nebrass
**Date:** 2026-04-26
**Status:** In progress (Step 3 of 13 complete)

## Executive Summary

Zorn Mesh is a single-binary local message broker and protocol adapter that lets AI coding agents — Claude Desktop, Cursor, Copilot, Gemini, plus any custom Python/TypeScript/Rust agent — discover each other, exchange typed messages, replay history, and be inspected on a single developer machine, without a network broker.

**Three agents on your laptop, talking. Trace anything. No broker.**

The 2026 protocol landscape has settled: MCP 2025-11-25 owns tool-calling, A2A v0.3 (with ACP merged in September 2025) owns agent-peer RPC, AGNTCY/SLIM is emerging for federated overlays. None solve local coordination. A developer running 3–5 agents in parallel today has zero infrastructure for them to coordinate — they cope with random ports, `/tmp` JSON files, and ad-hoc orchestration. When a multi-agent pipeline breaks at 2 a.m., they have nothing to look at, and the deepest pain follows mechanically: when no one can answer "what did the agents actually do," no one builds the next agent.

Zorn Mesh fills that gap. The bet: **MCP is the wire, NATS is the model, SQLite is the store, Rust is the runtime.** No new ideas — just the right ones, packaged for the developer machine. A privileged-by-uid broker daemon auto-spawns on first SDK connect (the `sccache` pattern), owning a Unix-domain socket and a single SQLite file at `~/.local/state/zornmesh/mesh.db`. Agents call `Mesh.connect()`, register, advertise capabilities, and immediately publish to topics, request from peers, subscribe with NATS-style hierarchical wildcards, and stream chunks — all over JSON-RPC 2.0 with LSP-style framing, byte-compatible with MCP so existing clients (Claude Desktop, Cursor) join the bus via `zornmesh stdio --as-agent <id>` without modification.

The strategic positioning is **witness, not just broker.** The audit log is testimony; `correlation_id` is a thread of memory; local-first is the authority of observation returned to the developer's own machine. The product refutes the assumption — baked into every multi-agent framework today — that internal agent behavior need not be observable. The antagonist is **the unobservable**; the archetypal need this product satisfies is the need to witness.

### What Makes This Special

- **Three-property compound, not three features.** Local-first by architecture (no broker to install) + MCP-superset wire (no opt-in friction; existing MCP clients work as-is via the stdio bridge) + single 22 MB Rust binary (no operations burden). Any two of those and you're a developer tool. All three at once and you're the SQLite of agent buses.
- **The killer feature is forensic.** `zornmesh trace <correlation_id>` reconstructs a multi-agent conversation in the terminal — Jaeger-quality forensics with zero infrastructure setup. The first time a developer pastes a UUID and watches the conversation rebuild, the agents stop being black boxes.
- **Symmetric capability model**, unlike MCP's hub-and-spoke. Both ends advertise both consumer and provider sets at handshake. Native peer-to-peer feel rather than tunneled.
- **Inspectable by design.** SQLite store, Unix-domain socket, JSON wire, OpenTelemetry tracing. `sqlite3`, `socat`, and `zornmesh tail` are first-class debug surfaces. The 2 a.m. on-call story is `sqlite3 mesh.db 'SELECT * FROM messages WHERE stream=? ORDER BY offset DESC LIMIT 50'`.
- **Honest reliability claims.** At-least-once delivery with mandatory idempotency keys, full-jitter exponential backoff, dead-letter queue, lease-based pull delivery. Exactly-once is explicitly rejected — Jepsen 2026 demonstrated it is unattainable even for NATS JetStream, and lying about it erodes developer trust.
- **The conceptual moat is `mesh-trace/1.0`** — a published open standard for multi-agent conversation replay (correlation IDs, causality chains, message boundaries, agent identities), shipped *before* public MVP. When "how do I audit my agent mesh?" becomes the universal question (~6 months out), the answer should already have a name, and `zornmesh trace` should be the reference implementation.

## Project Classification

| Field | Value |
|---|---|
| Project type | `developer_tool` (with `cli_tool` + system-daemon characteristics) |
| Domain | `developer_infra` (informal — supersedes BMad CSV's `general`; complexity override required) |
| Complexity | `high` |
| Project context | `greenfield` |
| Primary job | Observability of a broken multi-agent system at 2 a.m. — "know exactly what happened, and when" |
| Primary persona (v0.1) | Individual developer (buyer = user) |
| Future persona (v0.5+) | Platform engineer at mid-size org adopting agent fleets (buyer ≠ user) |
| Launch gate | MCP-stdio bridge interoperating with at least one major host (Claude Desktop or Cursor) at v0.1 |
| Mandatory PRD addenda | Operations & Lifecycle section · Compliance & Regulatory section · Stakeholder Map section |

## Success Criteria

### Telemetry & Privacy Posture (load-bearing prerequisite)

User and business metrics depend on instrumentation. Privacy posture is a release-gating decision that ships in v0.1 — not a future enhancement.

- **Telemetry capability:** `mesh.telemetry` is a v0.1 deliverable. Schema: anonymized session id, event type, event timestamp, daemon version, OS family, SDK language, no envelope payloads, no agent ids, no `correlation_id`s, no user paths.
- **Privacy default: opt-out.** Telemetry collection is disabled by default. Activation requires explicit `ZORN_TELEMETRY=1` or first-run interactive consent. Disabled telemetry produces no network traffic — `zornmesh doctor` confirms this on demand.
- **Identity model:** persistent anonymous installation id (UUIDv7, generated at first run, written to `~/.config/zornmesh/install.id`, mode 0600). Survives daemon restarts; not tied to any external identity. Deleting the file is the user-side reset; reinstall regenerates. This identity model is what enables ships-twice rate to be measured.
- **No payload telemetry, ever.** Envelope contents, capability schemas, and trace bodies never cross the daemon boundary in telemetry events.
- **Public dashboard:** v0.1 ships a public dashboard at `https://zornmesh.dev/telemetry` showing aggregate metrics — transparency is the trust-restoration move. Corollary: every metric in this PRD that depends on telemetry is computed against the consenting cohort *and* the cohort size is published alongside.

### User Success — Two North Stars

The user is the developer running 3–5 agents on their laptop today. The PRD's classified primary job is *observability of a broken multi-agent system*; coordination is the prerequisite, observability is the hire. **Two North Stars, not one** — one for activation, one for retention.

- **Activation North Star: time-to-first-coordinated-message ≤ 600 seconds (p50).**
  Measured from `cargo install zornmesh` (or `npm i @zornmesh/sdk`) initiation to first envelope flow. **Survivorship-corrected:** instrumentation captures `install.attempt.started` AND `coordination.first_envelope.observed`; p50 is computed over *attempts*, not over successes. "No completion within 900 s of attempt-start" is a *failure observation*, not a missing observation. The cohort size is published alongside the percentile.
- **Retention North Star: time-to-first-trace ≤ 30 seconds (p50).**
  Measured from "user encounters a multi-agent failure scenario" (proxied by `correlation_id` paste detection in the CLI) to a complete `zornmesh trace` reconstruction in the terminal. The agents are no longer black boxes. This is the load-bearing metric for the *primary job*.
- **7-day active trace rate ≥ 40% at v0.5.**
  Of activated users (those who completed activation North Star), the percentage running at least one `zornmesh trace` query in their first 7 days. Behavioral signal: the hire being re-hired. Low number = road to a destination people aren't visiting.
- **Reverse-NPS as the sentiment metric** (replacing standard NPS).
  Single question: "Would you build your next multi-agent workflow without Zorn Mesh?" Failure case: "yes" rate > 40% among active developers (means we're optional, not load-bearing). Asked at month 1 and month 6 post-activation. Survey-bias caveat: respondents are self-selected; cohort size and response rate published alongside.
- **Bridge adoption ≥ 30% at v0.5.**
  Of active developers, the percentage connecting at least one MCP-host-backed agent (Claude Desktop, Cursor, VS Code Copilot) to the bus via `zornmesh stdio --as-agent`. Wedge-mechanism signal. **Caveat:** if the stdio bridge becomes the default integration path for any major host, the metric inflates trivially; the threshold may need re-baselining.
- **Ships-twice rate ≥ 60% at v0.5.**
  Of activated users who shipped one multi-agent workflow with the tool (defined as `Mesh.connect()` followed by ≥10 envelopes flowing across ≥2 agents over a 7-day window), the percentage who ship a second workflow within 30 days. Trust-restoration proxy.
- **Cut at this draft (vanity proxies, not value signals):** "onboarding completion ≥ 90%" (measures clicked-the-button, not got-the-value). Standard NPS (noise for a v0.1 dev tool with tiny sample).

### Business Success — Adoption ladder with forcing-event calibration

Zorn Mesh is open-source, developer-tool-shaped; "business success" means ecosystem adoption and integration depth, not direct revenue. The adoption ladder below is calibrated against comparable infra-tool launches with no prior brand and non-trivial integration surface (`just`, `sqlx`, `sccache`, Litestream, NATS-as-CLI). Targets without a named forcing event default to the conservative end of that empirical range.

| Tier | Metric | v0.1 (10 wk) | v0.5 (TBD) | v1.0 (TBD) | Forcing event assumed |
|---|---|---|---|---|---|
| Adoption | Active developers (telemetry-consenting cohort) | **20–80 / 30 days** at conservative; 100/30d if forcing event lands | **400–800 / wk** baseline; 1k/wk if forcing event lands | 5k / wk | v0.5 needs: ≥1 major-runtime adapter OR a notable framework integration (LangChain/LlamaIndex/AutoGen) OR a viral demo |
| Adoption | Major-runtime adapters (default-config inclusion) | 0 | **≥ 1 with default-config inclusion** (binary metric — depth, not breadth) | ≥ 3 | v0.5 needs: at least one runtime vendor co-authored or endorsed integration |
| Community | External contributors (merged non-trivial PRs) | tracking only | ≥ 10 | ≥ 50 | none |
| Standards | "External engineering team has made a public build-or-announce decision based on `mesh-trace/1.0`" (binary kill-switch) | not yet evaluated | **≥ 1 by month 6** (kill-switch threshold) | ≥ 5 | v0.5 needs: vendor-partnership precondition (≥1 runtime co-authoring or endorsing the spec); without it, the metric is reset to "tracking only" |
| Standards | "Citation" of `mesh-trace/1.0` (precise definition: spec referenced in a public repo's documentation, a published blog post, OR a conference talk abstract) | tracking only | ≥ 3 *if* vendor-partnership precondition met; else "tracking only" | ≥ 20 *if* working-group relationship established; else strategy reframed | v0.5 needs: outreach to MCP TSC, A2A working group, OR AGNTCY maintainers |
| Ecosystem | 3rd-party SDK ports (additional languages) | 0 | tracking only | ≥ 5 | none |
| Stakeholder | Enterprise platform team with ≥1 merged non-trivial contribution | none | tracking only | **≥ 1** (leading indicator for monetization track) | none |
| Stakeholder | Public acknowledgment in MCP / A2A / AGNTCY working-group artifact (FAQ, spec reference, meeting notes) | none | **≥ 1** (leading indicator for standards-tier credibility) | ≥ 3 | v0.5 needs: at least one issue opened in MCP/A2A spec repos referencing `mesh-trace/1.0` |

**Conceptual-moat kill-switch (re-stated operationally):**
By **month 6 post-launch**, has any external engineering team made a *public build-or-announce decision* based on `mesh-trace/1.0`? Examples that satisfy: a competing tool announces support; a major IDE ships an integration; an A2A or MCP working-group document references the standard; a third-party SDK is announced. Citations are the trail; this is the predator.

If the answer at month 6 is **no**, the conceptual-moat strategy has failed and the team pivots — either to product-moat-only mode or a different conceptual play. This kill-switch is binary, fast, and unambiguous; it does not depend on counting things that are gameable.

### Technical Success — Release-gating CI gates

Every item below is a release-gating CI gate. The MVP cannot ship until they all pass.

- **Codec correctness:** every `MessageType` envelope round-trips through `proptest` between JSON, Protobuf, and Rust types preserving byte-equality. CI gate.
- **Cross-SDK byte-equivalence — golden corpus.** **Required addition to `buf breaking`.** A 50–100 envelope golden corpus (`/conformance/cross-sdk/`) covering: realistic field populations, edge values (zero timestamps, empty strings, max-length agent ids, repeated fields with zero elements, unknown enum values, optional-with-default-value vs absent). Each SDK encodes the corpus and asserts byte-equivalence. `buf breaking` is a schema linter; this corpus is the behavior test. Both required.
- **Lease-reaper safety:** `turmoil` simulation with **≥100 distinct seeds** (not a single fixed seed). Crashes uniformly distributed across lease state-machine transitions (pre-write / mid-write / post-write / pre-reap / mid-reap) using state-machine injection points. Explicit deterministic tests for crash-during-write (duplication risk) and crash-during-reap (loss risk). Concurrent-crash-pattern coverage (two agents crashing within the same reap interval). Total: zero envelope loss and zero envelope duplication outside the at-least-once contract across the full corpus. CI gate.
- **Schema migration cleanliness:** `cargo test --features migration-stress` applies every migration to a blank DB AND seeds known state at each prior schema version, verifies daemon startup at every version. CI gate.
- **Trace correctness (not just snapshot):** `zornmesh trace <correlation_id>` reconstructs a complete timeline for the 3-agent example with no gaps, no out-of-order spans, no missing causality edges. **Plus property tests** that inject partial-message loss, out-of-order delivery, and agent-crash-mid-trace and assert the trace output flags these conditions explicitly rather than silently rendering an incomplete picture.
- **Wire stability:** `buf breaking` against `main` on every PR; field-number reuse fails. Zero violations between v0.1 and v1.0 of `zorn.mesh.v0`.
- **Performance envelope** (split: steady-state + burst):
  - **Steady-state**: 1,000 msg/s with 1 KiB realistic-shape payloads (mix of `mesh.publish`, `mesh.request`, `STREAM_CHUNK` proportional to expected production traffic). p50 envelope latency < 5 ms; p99 < 50 ms.
  - **Burst**: 10,000 msg/s for 10-second windows with mixed payload sizes (token-stream shape: 4 KiB chunks/s from a single source with 32-byte payloads; concurrent multi-stream interleave). p99 < 100 ms; zero envelope loss; queue depth bounded.
  - **Resource ceiling**: idle daemon RSS 5–15 MiB; cold-start auto-spawn < 200 ms.
- **Supply-chain integrity:** `cargo-deny` (license + advisory audit), CISA SBOM emitted on every release tag, all release artifacts signed with Sigstore cosign.
- **Compliance posture:** SOC 2 Type II attestation gating the platform-engineer go-to-market track at v0.5+ (informational at v0.1; formal target at v1.0).

### Measurable Outcomes

| Tier | Metric | v0.1 (10 wk) | v0.5 (TBD) | v1.0 (TBD) |
|---|---|---|---|---|
| User (activation NS) | time-to-first-coordinated-message (p50, attempt-anchored) | < 600 s | < 300 s | < 120 s |
| User (retention NS) | time-to-first-trace (p50) | < 30 s | < 15 s | < 10 s |
| User | 7-day active trace rate | tracking only | ≥ 40% | ≥ 60% |
| User | Reverse-NPS "build-without-us" rate | tracking only | ≤ 40% | ≤ 25% |
| User | Bridge adoption | tracking only | ≥ 30% | ≥ 50% |
| User | Ships-twice rate | tracking only | ≥ 60% | ≥ 75% |
| Business | Active developers (consenting cohort) | 20–80 / 30 d | 400–800 / wk | 5,000 / wk |
| Business | Major-runtime adapters with default-config inclusion | 0 | ≥ 1 | ≥ 3 |
| Business | External contributors (merged PRs) | tracking only | ≥ 10 | ≥ 50 |
| Standards | `mesh-trace/1.0` build-or-announce kill-switch | published before MVP | ≥ 1 by month 6 | ≥ 5 |
| Standards | `mesh-trace/1.0` precise-definition citations | tracking only | ≥ 3 *iff* vendor-partnership precondition met | ≥ 20 *iff* working-group relationship |
| Stakeholder | Enterprise platform team contribution | none | tracking only | ≥ 1 |
| Stakeholder | Working-group artifact acknowledgment | none | ≥ 1 | ≥ 3 |
| Ecosystem | 3rd-party SDK ports | 0 | tracking only | ≥ 5 |
| Technical | p99 envelope latency (1k msg/s steady-state) | < 50 ms | < 25 ms | < 25 ms |
| Technical | p99 envelope latency (10k msg/s burst) | < 100 ms | < 75 ms | < 50 ms |
| Technical | Idle daemon RSS | 5–15 MiB | 5–15 MiB | 5–15 MiB |
| Technical | Cold-start auto-spawn | < 200 ms | < 200 ms | < 100 ms |
| Technical | Zero-CVE release | required | required | required |
| Technical | Cross-SDK golden-corpus byte-equivalence | required (≥50 envelopes) | required (≥100 envelopes) | required (production-corpus) |
| Compliance | SOC 2 Type II attestation | not required | informational | formal target |

## Product Scope

### MVP — Minimum Viable Product (v0.1, ~10 weeks)

**In scope:**

- Registration with capabilities; presence and heartbeats (5 s cadence, 15 s timeout).
- Request/reply with correlation IDs and cancellation; fire-and-forget events.
- Topic pub/sub with NATS-style hierarchical wildcards (`*`, `>`); direct messaging via `agent.<id>.inbox`.
- Streaming via `STREAM_CHUNK` envelopes correlated by `correlation_id`, terminated by `final=true`.
- Append-only message log with offset-based replay (`zornmesh replay <stream> --from-offset N`).
- Pull-based at-least-once delivery with leases (`mesh.fetch` / `mesh.ack` / `mesh.nack`), full-jitter exponential backoff, dead-letter queue.
- Idempotency via optional `dedup_id` (5-min window, partial unique index).
- OS-level trust: UID match + socket ACL; `peer_cred()` rejection on mismatch; abstract sockets refused.
- Structured OpenTelemetry tracing (traces + metrics + logs) with `mesh.<message_type> <destination>` span naming; W3C trace context inline in envelope; Prometheus loopback scrape on the daemon's HTTP admin surface.
- CLI surface: `zornmesh daemon`, `zornmesh stdio --as-agent <id>`, `zornmesh agents`, `zornmesh tail <topic>`, `zornmesh replay`, `zornmesh inspect`, `zornmesh doctor`, `zornmesh trace <correlation_id>`.
- Auto-spawn (`sccache` pattern) with `ZORN_NO_AUTOSPAWN=1` opt-out.
- **`mesh.telemetry` capability**, opt-out by default, anonymous-install-id model, public aggregate dashboard at `zornmesh.dev/telemetry`.
- Release artifacts: `cargo install zornmesh` (Rust binary, multi-arch), `npm i @zornmesh/sdk`. Linux + macOS only.
- **`mesh-trace/1.0` open standard published as an RFC-style document before public MVP** — the conceptual-moat move.
- All compliance baseline: cargo-deny clean, SBOM emitted, Sigstore cosign on releases.

**Explicitly out of scope at v0.1:**

- Per-agent cryptographic identity, signed envelopes, capability tokens.
- Multi-host federation, A2A bridge, AGNTCY/SLIM bridge, replication.
- Web dashboard. (Inspection is the CLI; dashboard is sugar.)
- Windows named-pipe support; Python SDK.
- Dynamic policy / RBAC / multi-tenant namespacing.

### Growth Features (v0.2 + v0.5)

**v0.2 (~6 weeks after v0.1):**

- Ed25519 signed envelopes; per-agent keys at `~/.config/zornmesh/keys/<agent_id>.ed25519` (mode 0600).
- Replay protection via per-envelope `id` (ULID/UUIDv7) + 5-min recent-ID set.
- Windows named-pipe support (`\\.\pipe\zornmesh-<sid>`).
- Python SDK (Pydantic v2 + protobuf runtime + `mypy --strict`).
- MCP-stdio bridge polish (compatibility with Claude Desktop, Cursor, VS Code Copilot).

**v0.5 (TBD):**

- Capability tokens via `biscuit-auth = "5"` — macaroons-style attenuation, ≤ 5-min TTL, audit-logged token serial.
- Axum-served single-page web dashboard at `localhost:9876` (live agent topology, message timeline, DLQ inspection, replay tool, trace viewer).
- A2A gateway: `zornmesh a2a-gateway --listen 127.0.0.1:9999`.
- Optional `fjall` hot-log split — only if v0.5 measured write-stalls demand it.

### Vision (v1.0+)

- Stable wire protocol with deprecation policy.
- Federation between Zorn Mesh instances on different machines via TLS-tunneled gRPC.
- AGNTCY/SLIM bridge if demand exists at month 12+ (defined: ≥3 production deployments asking, OR ≥1 paying customer).
- `mesh-trace/1.0` recognized as a deployable observability profile in ≥1 independent agent framework.
- 3rd-party SDK ecosystem: ≥5 SDK ports in additional languages.
