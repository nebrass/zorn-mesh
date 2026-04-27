# zorn-mesh — Project Context

## Project Goals

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

## Success Metrics

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

## Scope Boundaries

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

---
validationTarget: '_bmad-output/planning-artifacts/prd.md'
validationDate: '2026-04-27'
inputDocuments:
  - '_bmad-output/project-context.md'
  - '_bmad-output/planning-artifacts/product-brief-zorn-mesh.md'
  - '_bmad-output/planning-artifacts/product-brief-zorn-mesh-distillate.md'
  - '_bmad-output/planning-artifacts/ux-design-specification.md'
  - '_bmad-output/planning-artifacts/architecture.md'
validationStepsCompleted: ['step-v-01-discovery', 'step-v-02-format-detection', 'step-v-03-density-validation', 'step-v-04-brief-coverage-validation', 'step-v-05-measurability-validation', 'step-v-06-traceability-validation', 'step-v-07-implementation-leakage-validation', 'step-v-08-domain-compliance-validation', 'step-v-09-project-type-validation', 'step-v-10-smart-validation', 'step-v-11-holistic-quality-validation', 'step-v-12-completeness-validation']
validationStatus: COMPLETE
holisticQualityRating: '4/5 - Good'
overallStatus: 'Pass'
---

## Design Guidelines

Foundation

### 1.1 Design System Choice

zorn-mesh will use a **themeable utility/component stack** for the v0.1 local web UI:

- **React** for the frontend application model.
- **Bun** for package/runtime/build tooling, aligning with the architecture's TypeScript runtime direction.
- **Tailwind CSS** for design tokens, layout, spacing, color, and responsive styling.
- **shadcn/ui-style composition with Radix-style accessible primitives** for dialogs, popovers, dropdowns, tabs, tooltips, command menus, toasts, form controls, and other interaction-heavy components.
- **Custom zorn-mesh components** for domain-specific surfaces such as live agent roster, message timeline, trace detail, delivery-state badges, local daemon status, and safe direct/broadcast composer.

This is not an off-the-shelf visual identity. It is a pragmatic foundation: accessible primitives and utility styling for speed, with custom domain components for the parts that make zorn-mesh distinct.

### Rationale for Selection

This approach best matches the UX and product constraints:

- **Calm density:** Tailwind and composable primitives support a restrained, Linear-inspired interface without inheriting a heavy enterprise visual language.
- **Fast implementation:** shadcn/Radix-style components provide accessible building blocks while allowing the project to own the final component code and styling.
- **Developer-tool fit:** the stack supports keyboard-friendly workflows, command palettes, panels, tables, timelines, badges, inspectors, drawers, and toasts commonly needed in technical tools.
- **Trace-specific flexibility:** zorn-mesh needs custom timeline and evidence views that would be awkward to force into a generic enterprise design system.
- **Local/offline packaging:** the UI can be bundled as local static assets without relying on external CDNs or hosted design assets.
- **Bun alignment:** the design system does not introduce npm/pnpm/yarn expectations into the architecture; frontend tooling should remain compatible with the Bun-only TypeScript direction.
- **Accessibility baseline:** Radix-style primitives help establish accessible interaction behavior for dialogs, menus, focus handling, keyboard navigation, and screen-reader semantics.

Established enterprise systems such as MUI or Ant Design would accelerate broad CRUD/admin UI work but risk visual and interaction weight that conflicts with the desired calm control-room feel. A fully custom design system would maximize fit but adds unnecessary v0.1 cost. Minimal custom CSS would reduce dependencies but increases risk around accessibility, consistency, and polish.

### Implementation Approach

The design system should be implemented as a small internal UI layer rather than a large abstract design-system project.

Recommended structure for downstream architecture/story work:

- **Foundational tokens:** color, typography, spacing, radius, shadow, borders, focus rings, semantic states, and motion duration.
- **Primitive wrappers:** project-owned wrappers around accessible primitives for buttons, inputs, dialogs, popovers, tooltips, tabs, menus, toasts, badges, and panels.
- **Layout primitives:** app shell, split panes, resizable panels, sticky status header, side navigation or roster column, detail drawer/panel, and empty/error state containers.
- **Domain components:** agent roster item, agent status badge, message row, trace timeline node, causal-link indicator, delivery-state badge, broadcast recipient preview, daemon status indicator, CLI command copy block, and safe composer.
- **State-specific components:** stale state, reconnecting state, daemon unavailable state, missing trace state, partial broadcast failure, dead-letter marker, replay marker, and local-only trust indicator.
- **Accessibility checks:** keyboard navigation, focus trapping, ARIA labels for status/timeline elements, visible focus, color contrast, reduced-motion handling, and screen-reader text for icon-only states.

### Customization Strategy

The visual direction is a **calm dark-first developer console with light mode supported**. Dark mode should be the primary designed experience because the target users are developer-tool users working with dense state, traces, and debugging flows. Light mode should be supported through the same token system, not added as an afterthought.

Customization priorities:

- **Visual tone:** calm, precise, trustworthy, low-noise, and technical without looking like a raw terminal.
- **Color system:** semantic colors for healthy, stale, busy, warning, error, failed, delivered, acknowledged, replayed, and dead-letter states. Avoid relying on color alone.
- **Typography:** readable mono usage for IDs, subjects, timestamps, sequence numbers, CLI commands, and payload fragments; readable sans usage for labels, navigation, and explanatory text.
- **Motion:** subtle live-state updates and transitions that communicate change without making the UI feel noisy or unstable.
- **Density:** compact enough for agent/message inspection, but not dashboard-dense; progressive disclosure should keep advanced protocol details accessible but secondary.
- **Trust cues:** local-only status, daemon health, session protection, offline/bundled status, and state freshness indicators should feel integrated into the UI chrome.
- **Domain identity:** zorn-mesh should feel like a witness/control room for local agents, not a generic admin panel.
