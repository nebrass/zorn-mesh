---
stepsCompleted: ['step-01-validate-prerequisites', 'step-02-design-epics', 'step-03-create-stories', 'step-04-final-validation']
inputDocuments:
  - "_bmad-output/planning-artifacts/prd.md"
  - "_bmad-output/planning-artifacts/architecture.md"
  - "_bmad-output/planning-artifacts/ux-design-specification.md"
workflow: bmad-create-epics-and-stories
mode: create
status: complete
project_name: zorn-mesh
lastUpdated: '2026-04-27'
---

# zorn-mesh - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for zorn-mesh, decomposing the requirements from the PRD, UX Design if it exists, and Architecture requirements into implementable stories.

## Requirements Inventory

### Functional Requirements

FR1: An Agent can publish an envelope to a subject with at-least-once delivery semantics.

FR2: An Agent can subscribe to a subject pattern (exact, prefix, or hierarchical wildcard) and receive matching envelopes.

FR3: An Agent can issue a request to a target agent and receive exactly one correlated reply, or a typed timeout / cancellation result.

FR4: An Agent can fetch envelopes via pull-based delivery with explicit lease acquisition and renewal.

FR5: An Agent can stream a response to a request as a sequence of chunks bounded by an explicit byte-budget window.

FR6: An Agent can acknowledge or negative-acknowledge a delivered envelope, with a structured reason on negative acknowledgement.

FR7: An Agent can cancel an in-flight request or stream by correlation ID.

FR8: An Agent can supply an idempotency key on any send-side operation, and the broker enforces at-least-once-with-deduplication semantics within a defined uniqueness window.

FR9: An Agent can establish a durable subscription that survives daemon restart and resumes from the last acknowledged envelope.

FR10: The broker can apply backpressure when a consumer reaches its configured queue bound or misses its acknowledgement/lease budget, with publisher-visible feedback and no silent drops unless explicitly configured.

FR11: An Agent can register an agent card declaring identity, version, and the set of capabilities it advertises and consumes.

FR12: An Agent can advertise capabilities symmetrically - the same capability primitives govern what an agent offers and what it consumes.

FR13: An Operator can mark a capability as high-privilege, restricting which agents may advertise or invoke it.

FR14: An Adopter can resolve the current set of registered agents and their advertised capabilities.

FR15: An Adopter can connect to the mesh from an SDK and have the daemon auto-spawn if not already running, without intervention.

FR16: An Operator can disable auto-spawn and require explicit daemon startup via an environment variable.

FR17: An Operator can start, stop, and query daemon status explicitly from the CLI.

FR18: The daemon enforces that exactly one instance owns the broker socket per machine, even under concurrent connection attempts.

FR19: The daemon refuses to run with elevated privileges (root / Administrator) and reports a named error.

FR20: The daemon performs a graceful drain on shutdown, bounded by an operator-configurable budget.

FR21: An Operator can run a self-diagnostic that reports daemon version, socket path, schema version, OTel reachability, signature verification, and trust posture.

FR22: The broker persists every envelope to a tamper-evident audit log durable across daemon and host restart.

FR23: The broker captures undeliverable envelopes or envelopes whose retry budget or TTL is exhausted into a dead-letter queue with structured failure reason.

FR24: A Developer can reconstruct the timeline of a conversation by correlation ID across all participating agents.

FR25: A Developer can re-deliver a previously-sent envelope from the audit log to support recovery scenarios.

FR26: A Developer can inspect persistence state (messages, dead letters, audit log, SBOM, schema version) with structured filters.

FR27: An Operator can configure retention policy by age, count, and capability class, with defaults of 24 h messages, 7 d DLQ, and 30 d audit log.

FR28: A Compliance Reviewer can verify audit-log tamper-evidence offline without daemon access.

FR29: The broker propagates W3C tracecontext across every wire operation without adopter intervention.

FR30: The broker emits OpenTelemetry metrics and traces conforming to a documented schema.

FR31: A Developer can live-tail envelopes by subject pattern as structured records.

FR32: A Developer can reconstruct a span tree for a request/reply or streaming exchange end-to-end.

FR33: A Host (e.g., Claude Desktop, Cursor) can connect to the mesh via an MCP-stdio bridge that exposes a registered agent.

FR34: An Adopter can register the same agent across multiple host connections and have the broker route to a single canonical identity.

FR35: When the connected host speaks only baseline MCP, the bridge exposes only mesh capabilities representable on the MCP wire and returns a named unsupported-capability result for the rest.

FR36: An Adopter can mark fields as secret and the secret value is redacted in all log, metric, trace, and audit-log emissions.

FR37: An Operator can verify that the running binary's signature matches the published Sigstore signature.

FR38: An Operator can retrieve the SBOM (CycloneDX) of the running binary at runtime.

FR39: An Adopter can never invoke a high-privilege capability without a corresponding operator-approved entry in the capability gate.

FR40: The broker rejects connections that do not satisfy the local socket permission model.

FR41: Every envelope carries the traceability fields required by the PRD's EU AI Act mapping evidence bundle: agent identity, capability invoked, timestamp, correlation ID, trace ID, and prior-message lineage.

FR42: A Compliance Reviewer can export an evidence bundle (audit log slice, SBOM, signature, configuration snapshot) for a specified time window.

FR43: A Subject Data Owner can request, via a documented procedure, the deletion of envelopes referencing their personal data, scoped to retention-policy obligations.

FR44: A Compliance Reviewer can map any envelope to a documented NIST AI RMF function/category from a versioned mapping table, with unmapped envelopes reported explicitly and manual overrides audit-logged.

FR45: A Developer or Operator can request structured (JSON) output on every read subcommand.

FR46: A Developer or Operator can run any subcommand non-interactively under a flag that fails fast rather than prompting.

FR47: A Developer can install shell completions for their shell from the CLI itself.

FR48: Every CLI subcommand exits with a stable, documented exit code distinguishing user error, daemon-unreachable, validation failure, permission denied, and not-found.

FR49: A Developer can launch the local web UI from the CLI and receive either an opened browser window or a protected loopback URL suitable for copy/paste.

FR50: A Developer can view connected agents live, including identity, status, capabilities summary, transport/source, warnings, and last-message recency.

FR51: A Developer can view a message/trace timeline ordered by daemon sequence, with browser receipt time visible only as secondary diagnostic metadata.

FR52: A Developer can inspect a selected trace event and see payload summary, causality, delivery state, timing, source/target agent, and associated CLI command where available.

FR53: A Developer can open a focused trace view for one correlation ID and understand the full multi-agent conversation without stitching logs by hand.

FR54: A Developer can send a direct message to one selected agent from the UI after reviewing target identity, capability summary, and payload preview.

FR55: A Developer can broadcast a message only after reviewing the recipient list, excluded/incompatible recipients, payload summary, and explicit confirmation.

FR56: A Developer can see per-recipient delivery outcomes for direct and broadcast sends, including queued, delivered, acknowledged, rejected, timed out, and dead-lettered states.

FR57: A Developer can refresh or reconnect the UI and recover the selected trace context after daemon backfill completes.

FR58: An Operator can see daemon/local trust state in the UI, including loopback-only status, session protection, socket path, schema version, and any stale/disconnected warning.

FR59: The UI can display copyable CLI handoff commands for trace, inspect, replay, agents, doctor, and audit operations represented in the current context.

FR60: Human-originated UI sends create auditable records linked to actor/session, target recipients, trace/correlation ID, payload summary, and delivery outcome.

FR61: An Adopter can build an agent in Rust or TypeScript at v0.1 (Python at v0.2), against an SDK that provides identical wire semantics across languages.

FR62: An Adopter can carry a per-call idempotency key, trace context, and timeout through every SDK method without manual plumbing.

### NonFunctional Requirements

NFR-P1: Cold start from SDK connect invocation on a clean machine to daemon readiness for local mesh traffic <= 200 ms at p95 on the v0.1 reference platform.

NFR-P2: Sustained routing >= 5,000 envelopes/sec end-to-end with persistence enabled in the three-agent topology; CI benchmark regression > 10% fails.

NFR-P3: Request/reply latency p50 <= 2 ms and p99 <= 20 ms for loopback request/reply with <= 4 KiB payload and persistence enabled.

NFR-P4: Single stream throughput >= 50 MiB/sec sustained on the reference machine with the configured 256 KiB byte-budget window.

NFR-P5: Daemon resident memory <= 256 MiB under nominal load (<= 50 connected agents, <= 5,000 env/sec, default retention).

NFR-P6: A blocked subscriber surfaces backpressure to publishers within 100 ms; the broker does not silently buffer beyond an operator-configurable per-subscription bound.

NFR-P7: After daemon readiness, a local UI session renders currently connected agents within 2 seconds for the 3-agent fixture.

NFR-P8: Opening a trace with <= 500 events renders ordered timeline and selected-event detail within 1 second on the reference machine.

NFR-P9: Direct or 3-recipient broadcast sends display terminal per-recipient outcomes within 5 seconds unless an explicit agent timeout policy exceeds that budget.

NFR-S1: The agent data plane listens only on local IPC; the local web UI/API listener exists only when explicitly launched and binds loopback only.

NFR-S2: Socket file mode is 0600 and owned by the invoking user; permission deviations abort connection with a named error.

NFR-S3: Daemon refuses privileged execution and reports E_PRIVILEGED_REFUSED with explanatory text.

NFR-S4: Secret values never appear in stdout, stderr, logs, OTel attributes, audit payload fields, dead letters, or inspect output; lint gates reject unredacted emissions.

NFR-S5: Every release artifact carries a Sigstore signature and CycloneDX SBOM; doctor displays both; source install builds an SBOM.

NFR-S6: High-privilege capabilities are deny-by-default unless allow-listed, with checks at register-time and invoke-time.

NFR-S7: Every inbound envelope is validated against the canonical envelope schema before processing; malformed envelopes route to structured-error sinks, never subscribers.

NFR-S8: The daemon performs no outbound network call at runtime except an operator-configured OTel exporter that is off by default.

NFR-S9: The local UI requires a per-launch session token; state-changing requests require CSRF protection and origin checks; invalid protection fails closed.

NFR-S10: v0.1 UI refuses 0.0.0.0, non-loopback, LAN, or public hostname binding with named error and remediation.

NFR-S11: Local UI loads no CDN scripts, remote fonts, analytics, images, or remote config at runtime; browser E2E fails on external requests.

NFR-S12: Browser-visible payload summaries follow the same secret redaction rules as CLI, logs, traces, and audit output.

NFR-R1: Under 10 concurrent cold SDK connects, exactly one daemon process exists 95% of the time across 1,000 CI iterations and 100% across 10 manual runs.

NFR-R2: Every envelope acknowledged to a publisher is recoverable by replay after daemon SIGKILL and clean restart.

NFR-R3: SQLite WAL recovery completes <= 2 seconds for a database holding 7 days of default-retention audit log on the reference machine.

NFR-R4: On SIGTERM, daemon stops new connections, drains in-flight requests within shutdown budget, writes over-budget work to dead-letter, and exits with documented code.

NFR-R5: On SQLITE_FULL, daemon enters read-degraded mode: reads continue, new publishes return E_PERSIST_FULL synchronously, doctor reports condition, daemon does not crash.

NFR-R6: Forward-only schema migrations apply atomically at startup; failure leaves pre-migration state and daemon refuses to start with structured error.

NFR-R7: After browser disconnect, daemon restart, or tab refresh, UI backfills missed events by daemon sequence before marking view current.

NFR-SC1: Zorn Mesh targets single-machine deployment; > 200 connected agents per daemon or > 50,000 env/sec is non-goal at v0.1.

NFR-SC2: Default per-subject subscriber cap is 256 and per-machine total subscription cap is 4,096; cap violations return typed errors.

NFR-SC3: Subjects support up to 8 levels and <= 256 bytes; matching across 4,096 active subscriptions is sub-millisecond in CI benchmark.

NFR-SC4: Single envelope payload <= 8 MiB and streaming chunks <= 256 KiB; oversize envelopes are rejected with E_ENVELOPE_TOO_LARGE.

NFR-SC5: Default retention keeps messages 24 h, DLQ 7 d, audit 30 d; at 5,000 env/sec default on-disk footprint stays within 12 GiB and sweeps are incremental.

NFR-SC6: Metric label cardinality is capped by ZORN_METRICS_MAX_LABEL_VALUES; high-cardinality values are trace exemplars, not metric labels.

NFR-C1: v0.1 supports Linux glibc >= 2.31, Linux musl, and macOS >= 13 on x86_64 and aarch64 with signed/SBOM-attached artifacts.

NFR-C2: Rust and TypeScript SDKs ship at v0.1 with documented lower-bound runtime/toolchain support; Python ships at v0.2.

NFR-C3: Wire compatibility checks reject breaking changes against previous tag; breaking changes require explicit process and migration notes.

NFR-C4: MCP-stdio bridge passes conformance fixtures for Claude Desktop and Cursor on the v0.1 platform matrix.

NFR-C5: CLI honors NO_COLOR=1 and disables ANSI on non-TTY; JSON output is byte-identical regardless of TTY.

NFR-C6: Local UI targets current stable Chromium, Firefox, and Safari/WebKit on v0.1 desktop/laptop platforms; tablet/mobile are functional fallbacks.

NFR-C7: UI remains usable with network disabled after local loopback session is established; all application assets ship with the release artifact.

NFR-O1: Every wire operation propagates W3C traceparent/tracestate without adopter intervention; conformance asserts one span per envelope per hop.

NFR-O2: Metrics and spans conform to docs/observability/schema.md; schema changes follow semver.

NFR-O3: Tracing on adds <= 5% to throughput at default sampling; tracing off adds <= 0.5%.

NFR-O4: doctor returns within 1 second on healthy daemon and within 3 seconds on failure modes with structured cause.

NFR-M1: Release builds respect SOURCE_DATE_EPOCH where toolchain permits; identical inputs produce byte-identical binaries on reference host.

NFR-M2: Line coverage >= 70% per crate/package and branch coverage >= 90% on routing-critical modules.

NFR-M3: Every PRD MUST/MUST NOT rule is anchored to at least one conformance fixture and CI verifies fixture coverage by tag.

NFR-M4: Deterministic concurrency, distributed-scenario, and property-based tests cover routing core and wire-codec round-trips; flaky tests are quarantined with issue links.

NFR-M5: Public API documentation is machine-enforced for every SDK; missing public doc strings fail CI.

NFR-M6: Release pipeline supports independent semver releases for Rust, TS, and future Python SDKs.

NFR-M7: Browser E2E fixtures cover first open, live roster, trace inspection, safe direct send, safe broadcast, reconnect/backfill, keyboard-only navigation, and external-request blocking.

NFR-CA1: audit_log rows are hash-chained; offline audit verify detects any single-row tamper with probability >= 1 - 2^-50.

NFR-CA2: Retention deletes occur within 24 hours of threshold and are audited as RETENTION_PURGE records with capability-class breakdowns.

NFR-CA3: audit export emits audit-log slice, SBOM, signature, and sanitized config snapshot within 5 minutes for a 7-day window on the reference machine.

NFR-CA4: GDPR-style deletion requests redact personal-data fields while preserving correlation IDs and audit-log integrity with a proof record.

NFR-CA5: Every direct and transitive dependency appears in the CycloneDX SBOM; CI fails on unaccounted dependencies.

NFR-A11Y1: Core UI flows meet WCAG AA contrast and legibility requirements in dark-first mode and light mode.

NFR-A11Y2: Agent roster, trace timeline, detail panel, safe composer, broadcast confirmation, and command-copy controls are keyboard-operable with visible focus.

NFR-A11Y3: Trace timelines expose ordered structure, selected state, delivery state, and causality labels without relying on visual position alone.

NFR-A11Y4: Live-update animations respect prefers-reduced-motion; essential state changes remain perceivable without animation.

NFR-A11Y5: Agent status, delivery outcome, stale/disconnected state, warnings, and errors use text/icon/shape in addition to color.

NFR-A11Y6: CLI accessibility remains intact through NO_COLOR and stable JSON output so automation and assistive tooling are not forced through the UI.

### Additional Requirements

AR1: Use a custom zorn-mesh workspace scaffold; do not adopt a generic Rust CLI/web starter as the implementation base.

AR2: First implementation story must produce a compileable, testable scaffold plus a thin vertical smoke path, not folder-only scaffolding.

AR3: Root Rust workspace uses Rust edition 2024, resolver 3, Tokio as the only async runtime, and a stable MSRV policy.

AR4: Establish crate boundaries for zornmesh-proto, zornmesh-core, zornmesh-store, zornmesh-broker, zornmesh-rpc, zornmesh-sdk, zornmesh-daemon, zornmesh-cli, and xtask.

AR5: Keep dependency direction acyclic: lower domain/protocol crates must not depend on daemon, CLI, broker internals, or SDK edges.

AR6: TypeScript SDK lives under sdks/typescript and uses Bun only for package, runtime, build, and test tooling.

AR7: Python is a v0.2 boundary only; do not create fake v0.1 Python package code or failing placeholder tests.

AR8: No core runtime dependency may require NATS, Redis, Kafka, RabbitMQ, Docker, cloud SDKs, or an external database.

AR9: Create conformance fixtures early for protocol, persistence, security, observability, CLI output, MCP, AgentCard, error registry, and meta/forbidden-pattern rules.

AR10: Create CLI golden-output fixtures early for root help, trace help, trace success JSON, trace not-found output, tail NDJSON, and doctor healthy JSON.

AR11: Use just as the human task entrypoint and cargo xtask for generation, fixture validation, conformance orchestration, release preflight, and cross-language checks.

AR12: Internal mesh framing uses [length:u32 BE][frame_type:u8][payload], with frame length validated before allocation.

AR13: ACK/NACK, stream lifecycle, ping/pong, cancellation, flow control, and capability probes use a dedicated control-frame tier.

AR14: Explicitly model transport ACK, durable ACK, and delivery ACK; SDK retry behavior must document which ACK layer it observes.

AR15: MCP bridge compatibility targets protocol version 2025-11-25 with pinned conformance fixtures.

AR16: AgentCard identity metadata targets A2A 1.0.0; when older shapes are normalized, store raw and canonical forms.

AR17: TypeBox owns capability JSON Schema, Protobuf owns internal envelope/registry/persistence/audit models, and JSON-RPC remains the external method envelope.

AR18: SQLite via sqlx is the only v0.1 persistence engine; use WAL, a constrained writer path, reader pool, forward-only migrations, and ACK-after-commit durability.

AR19: Required persistence domains include agents, capabilities, messages, delivery leases, idempotency keys, subscriptions, stream state, DLQ, audit hash chain, trace indexes, retention/redaction markers, and schema migration state.

AR20: Migrations are forward-only, unknown future schemas refuse startup, and migration locks prevent concurrent daemon races.

AR21: No external cache; in-memory route/registry/schema caches must rebuild deterministically after restart and remain explainable through diagnostics.

AR22: v0.1 authentication trust anchor is local OS identity through UDS peer credentials; socket permissions and privilege refusal are mandatory.

AR23: No default TCP listener is allowed; Prometheus metrics are explicit opt-in loopback only with protection.

AR24: Stable product error registry lives in zornmesh-core and is consumed by daemon, CLI, Rust SDK, TypeScript SDK, JSON-RPC, and fixtures.

AR25: CLI human output, JSON, NDJSON, stderr wording, and exit codes are architectural contracts backed by fixtures.

AR26: Config precedence is CLI flag > environment variable > local config file > default; remote config is forbidden.

AR27: Structured logs are local and secret-redacted; OTel traces/metrics are available but export is disabled by default.

AR28: Release artifacts use zornmesh naming and include signed binaries plus SBOMs; Linux/macOS are v0.1 targets.

AR29: v0.1 remains single-machine and single-user scoped; no distributed broker/federation.

AR30: Architecture currently contains stale "no GUI/frontend/static web assets in v0.1" statements. The validated PRD and UX spec supersede those statements for v0.1 local web companion UI scope, so epics must include an architecture-alignment/update story or explicit supersession note before UI implementation stories.

### UX Design Requirements

UX-DR1: Implement the local web UI with React, Bun, Tailwind CSS, shadcn/ui-style composition, and Radix-style accessible primitives.

UX-DR2: Establish foundational design tokens for color, typography, spacing, radius, shadows, borders, focus rings, semantic states, and motion duration.

UX-DR3: Implement a dark-first graphite/charcoal visual palette with electric blue selection/action accents and cyan local-trust accents.

UX-DR4: Provide semantic state colors for success, warning, error, and neutral states, with light-mode token support.

UX-DR5: Use modern sans-serif typography for UI text and readable monospace typography for agent IDs, correlation IDs, subjects, timestamps, CLI commands, and payload metadata.

UX-DR6: Build a compact split-pane local console layout with roster/navigation, timeline, selected detail panel, composer, and persistent daemon/session status.

UX-DR7: Preserve timeline readability as the primary layout priority and use detail-on-demand for payload metadata, causal links, CLI commands, and protocol fields.

UX-DR8: Ensure WCAG AA contrast, visible focus, keyboard navigation, accessible labels, reduced-motion support, and non-color-only status communication.

UX-DR9: Provide project-owned primitive wrappers for buttons, inputs, dialogs, popovers, tooltips, tabs, menus, toasts, badges, panels, and layout primitives.

UX-DR10: Implement Trace Timeline with daemon sequence, timestamp, event summary, sender/recipient, causal marker, delivery state, status marker, expansion affordance, live-update behavior, gap/late/replay/dead-letter states, keyboard selection, and screen-reader sequence labels.

UX-DR11: Implement Trace Event Detail Panel with event summary, sender, recipients, subject, correlation ID, daemon sequence, timestamp, parent/child links, payload metadata, delivery outcome, suggested next action, and copyable CLI commands.

UX-DR12: Implement Causal Link Indicator for parent references, child count, caused-by/responds-to/replayed-from/broadcast fan-out labels, missing-parent and dead-letter states, and text labels for accessibility.

UX-DR13: Implement Delivery State Badge with consistent state language for pending, queued, accepted, delivered, acknowledged, rejected, failed, cancelled, replayed, dead-lettered, stale, and unknown.

UX-DR14: Implement Agent Roster Item with display name, stable ID, status, transport, last seen, capability summary, recent activity count, warning marker, filtering/highlighting behavior, and connected/idle/busy/stale/errored/disconnected/reconnecting states.

UX-DR15: Implement Agent Detail / Capability Card showing identity, transport, capabilities, subscriptions, recent traces, activity, trust indicators, permission indicators, MCP stdio/native SDK source, and high-privilege warnings.

UX-DR16: Implement Safe Message Composer with target mode selector, recipient selector/preview, message body, validation state, warning/confirmation area, send button, direct/broadcast distinction, and pending/partial/success states.

UX-DR17: Implement Broadcast Outcome List with per-recipient status badge, timing, failure reason, retry/inspect affordance, and full/partial/all-failed/pending/stale states.

UX-DR18: Implement Daemon / Local Trust Status with daemon health, loopback-only status, session protection, event freshness, current sequence, offline/bundled asset indicator, and healthy/starting/reconnecting/degraded/unavailable/stale/session-expired states.

UX-DR19: Implement CLI Command Copy Block with command, description, copy action, expected outcome, copied feedback, unavailable/requires-daemon/offline-audit states, and context-preserving command arguments.

UX-DR20: Implement Guided Recovery Panel for missing trace, stale agent, dead-letter event, partial broadcast failure, daemon unavailable, and audit warning scenarios.

UX-DR21: Treat agent status, delivery state, trace completeness, daemon health, and trust posture taxonomies as shared UX/API contracts across UI, CLI, logs, and SDK status.

UX-DR22: Build UI components with seeded fixtures and deterministic states so they can be tested before full daemon integration.

UX-DR23: Apply button hierarchy rules: one primary action per context, broad-impact actions require confirmation and scope preview, disabled actions explain why, loading actions avoid duplicate sends, and focus order stays logical.

UX-DR24: Implement feedback patterns where every state has text, critical states include explanation/next action, partial failure remains visible, and global toasts never replace persistent outcome display.

UX-DR25: Implement form validation for no recipient, stale recipient, invalid subject, unavailable daemon, unsafe scope, empty message, and distinct direct-vs-broadcast target patterns.

UX-DR26: Use Live Mesh Control Room as the default home with persistent roster/status, central timeline navigation, agent filtering/highlighting, trace detail navigation, and Focus Trace Reader return path.

UX-DR27: Implement empty/loading/error states for empty roster, no messages, missing trace, loading live state, daemon unavailable, and session expired.

UX-DR28: Implement search and filtering for agent ID/name, subject, correlation ID, delivery state, and time window, with visible removable filter chips and chronology-preserving results.

UX-DR29: Use dialogs for broad-impact confirmations, drawers/detail panels for inspection, popovers/tooltips for definitions, and avoid modal chains.

UX-DR30: Implement live update behavior where events append in daemon sequence order, late events are marked late/reconstructed, selected detail remains stable, and users can pause/focus trace views.

UX-DR31: Implement safe broadcast pattern with visually distinct broadcast mode, recipient count, previewable recipients, explicit confirmation copy naming scope, and per-recipient outcomes.

UX-DR32: Implement desktop/laptop-first responsive behavior: full three-pane layout at desktop, two-pane tablet behavior, one-pane mobile fallback, and no responsive mode may reorder trace events or hide delivery/failure state.

UX-DR33: Use Tailwind-aligned breakpoints: mobile below 768px, tablet 768-1023px, desktop 1024px+, wide desktop 1440px+.

UX-DR34: Test responsive behavior across desktop, tablet, mobile, Chromium, Firefox, Safari/WebKit, long technical strings, live updates while panes are collapsed, and selected detail stability.

UX-DR35: Test accessibility through automated checks, keyboard-only walkthroughs for critical journeys, screen-reader spot checks, reduced-motion checks, color-blindness checks, and manual no-color-only verification.

UX-DR36: Test UI states and fixtures for complete/partial/missing/reconstructed/live traces, stale/disconnected agents, reconnecting daemon, session expired, daemon unavailable, direct send success, broadcast success, broadcast partial failure, validation-blocked send, late event arrival, and backfill.

### FR Coverage Map

FR1: Epic 1 - First Local Mesh and SDK Bootstrap (basic publish path).

FR2: Epic 1 - First Local Mesh and SDK Bootstrap (basic subscribe path).

FR3: Epic 2 - Reliable Agent Coordination (request/reply).

FR4: Epic 2 - Reliable Agent Coordination (pull delivery and leases).

FR5: Epic 2 - Reliable Agent Coordination (streaming).

FR6: Epic 2 - Reliable Agent Coordination (ACK/NACK semantics).

FR7: Epic 2 - Reliable Agent Coordination (cancellation).

FR8: Epic 2 - Reliable Agent Coordination (idempotency).

FR9: Epic 2 - Reliable Agent Coordination (durable subscriptions).

FR10: Epic 2 - Reliable Agent Coordination (backpressure).

FR11: Epic 3 - Agent Identity, Capabilities, and Host Bridges (agent card registration).

FR12: Epic 3 - Agent Identity, Capabilities, and Host Bridges (symmetric capabilities).

FR13: Epic 3 - Agent Identity, Capabilities, and Host Bridges (high-privilege marking).

FR14: Epic 3 - Agent Identity, Capabilities, and Host Bridges (agent/capability resolution).

FR15: Epic 1 - First Local Mesh and SDK Bootstrap (SDK connect auto-spawn).

FR16: Epic 1 - First Local Mesh and SDK Bootstrap (auto-spawn opt-out).

FR17: Epic 1 - First Local Mesh and SDK Bootstrap (daemon start/stop/status).

FR18: Epic 1 - First Local Mesh and SDK Bootstrap (single-daemon ownership).

FR19: Epic 1 - First Local Mesh and SDK Bootstrap (privilege refusal).

FR20: Epic 1 - First Local Mesh and SDK Bootstrap (graceful drain).

FR21: Epic 1 - First Local Mesh and SDK Bootstrap (doctor/self-diagnostic).

FR22: Epic 4 - Forensic Persistence, Trace, and Recovery (durable audit log).

FR23: Epic 4 - Forensic Persistence, Trace, and Recovery (dead-letter queue).

FR24: Epic 4 - Forensic Persistence, Trace, and Recovery (timeline reconstruction).

FR25: Epic 4 - Forensic Persistence, Trace, and Recovery (redelivery/replay).

FR26: Epic 4 - Forensic Persistence, Trace, and Recovery (structured persistence inspection).

FR27: Epic 4 - Forensic Persistence, Trace, and Recovery (retention policy).

FR28: Epic 4 - Forensic Persistence, Trace, and Recovery (offline tamper verification).

FR29: Epic 4 - Forensic Persistence, Trace, and Recovery (tracecontext propagation).

FR30: Epic 4 - Forensic Persistence, Trace, and Recovery (OTel metrics/traces schema).

FR31: Epic 4 - Forensic Persistence, Trace, and Recovery (live tail).

FR32: Epic 4 - Forensic Persistence, Trace, and Recovery (span tree reconstruction).

FR33: Epic 3 - Agent Identity, Capabilities, and Host Bridges (MCP-stdio bridge).

FR34: Epic 3 - Agent Identity, Capabilities, and Host Bridges (canonical identity across host connections).

FR35: Epic 3 - Agent Identity, Capabilities, and Host Bridges (baseline MCP graceful limitation).

FR36: Epic 3 - Agent Identity, Capabilities, and Host Bridges (secret redaction).

FR37: Epic 5 - Compliance, Audit, and Release Trust Evidence (signature verification).

FR38: Epic 5 - Compliance, Audit, and Release Trust Evidence (SBOM retrieval).

FR39: Epic 3 - Agent Identity, Capabilities, and Host Bridges (high-privilege allowlist enforcement).

FR40: Epic 3 - Agent Identity, Capabilities, and Host Bridges (local socket permission rejection).

FR41: Epic 5 - Compliance, Audit, and Release Trust Evidence (EU AI Act traceability fields).

FR42: Epic 5 - Compliance, Audit, and Release Trust Evidence (evidence bundle export).

FR43: Epic 5 - Compliance, Audit, and Release Trust Evidence (personal-data deletion procedure).

FR44: Epic 5 - Compliance, Audit, and Release Trust Evidence (NIST AI RMF mapping).

FR45: Epic 1 - First Local Mesh and SDK Bootstrap (structured JSON read output).

FR46: Epic 1 - First Local Mesh and SDK Bootstrap (non-interactive fail-fast mode).

FR47: Epic 1 - First Local Mesh and SDK Bootstrap (shell completions).

FR48: Epic 1 - First Local Mesh and SDK Bootstrap (stable exit codes).

FR49: Epic 6 - Local Web Control Room and Safe Intervention (UI launch/protected URL).

FR50: Epic 6 - Local Web Control Room and Safe Intervention (live connected-agent view).

FR51: Epic 6 - Local Web Control Room and Safe Intervention (daemon-sequence timeline).

FR52: Epic 6 - Local Web Control Room and Safe Intervention (trace event detail).

FR53: Epic 6 - Local Web Control Room and Safe Intervention (focused trace view).

FR54: Epic 6 - Local Web Control Room and Safe Intervention (safe direct send).

FR55: Epic 6 - Local Web Control Room and Safe Intervention (safe broadcast).

FR56: Epic 6 - Local Web Control Room and Safe Intervention (per-recipient delivery outcomes).

FR57: Epic 6 - Local Web Control Room and Safe Intervention (UI reconnect/backfill).

FR58: Epic 6 - Local Web Control Room and Safe Intervention (daemon/local trust state).

FR59: Epic 6 - Local Web Control Room and Safe Intervention (CLI handoff commands).

FR60: Epic 6 - Local Web Control Room and Safe Intervention (auditable UI sends).

FR61: Epic 1 - First Local Mesh and SDK Bootstrap (Rust/TypeScript SDK agent build path).

FR62: Epic 2 - Reliable Agent Coordination (per-call idempotency, trace context, timeout).

## Traceability and Contract Anchors

### Story traceability label semantics

Story-level FR references use these labels:

- **Implemented:** the story delivers user-observable behavior for the FR.
- **Supported:** the story creates scaffold, contracts, fixtures, or dependencies required by later FR delivery.
- **Gated:** the story establishes a release, safety, conformance, or architecture gate before an FR can be claimed complete.
- **Verified:** the story proves already-implemented FR behavior through fixtures, benchmarks, release checks, browser tests, or audit evidence.

Every story must retain explicit FR IDs in this format:

`**FR traceability:** Implemented: FRx; Supported: FRy; Gated: FRz; Verified: FRn.`

Omit empty categories. Only **Implemented** counts as delivered FR scope; the other labels preserve Step 4 traceability without overclaiming implementation.

### Pinned cross-story contract ownership

| Contract | Owning story/gate | Required pin |
|---|---|---|
| Product error registry | Stories 1.6, 2.1 | Registry lives in `zornmesh-core`; entries include code, category, retryable flag, safe detail shape, CLI exit mapping, JSON-RPC error mapping, and fixture ID. |
| Canonical envelope schema | Stories 1.4, 4.1, 5.2 | Pin schema version, message ID, subject, source/target agent, capability ID/version, correlation ID, trace context, idempotency key, timestamp, prior-message lineage, payload metadata, redaction metadata, and delivery state. |
| Subject grammar | Story 1.4 | Pin exact, prefix, and hierarchical wildcard grammar; level/byte limits; reserved prefixes; escaping/validation rules; positive and negative fixtures. |
| Internal frame definitions | Stories 2.1, 2.5 | Pin `[length:u32 BE][frame_type:u8][payload]`, max length validation before allocation, data/control frame enum, and ACK/NACK/ping/pong/cancel/flow-control/capability-probe frames. |
| AgentCard fields | Story 3.1 | Pin A2A 1.0.0 profile fields: stable ID, display name, version, source/transport, raw form, canonical form, schema version, capability references, and trust metadata. |
| Capability schema ownership | Story 3.2 | TypeBox owns external capability JSON Schema; Protobuf owns internal envelope/registry/persistence/audit models; JSON-RPC remains the external method envelope. |
| Idempotency window | Story 2.4 | Pin dedupe scope as `(agent_id, operation_kind, idempotency_key)`, default window as 24h or message-retention window whichever is shorter, persisted retry state, and expired-window behavior. |
| Delivery-state taxonomy | Stories 2.1, 6.1 | Pin stable state names: pending, queued, accepted, durable_accepted, delivered, acknowledged, rejected, timed_out, cancelled, failed, retrying, backpressured, replayed, dead_lettered, stale, unknown. |
| Retention-gap schema | Stories 4.9, 5.3, 6.8 | Pin gap ID, affected sequence/time range, reason, retention policy version, purged-at time, capability-class breakdown, evidence impact, and suggested next action. |
| Redaction marker | Stories 3.5, 5.4 | Pin machine shape and display form for redacted values, including marker ID, reason/category, policy version, proof record ID when applicable, and "raw value never recoverable from marker" rule. |
| Audit record shape | Stories 4.1, 5.2, 5.3 | Pin audit ID, sequence, previous hash, record hash, actor/agent, action, subject/capability, correlation ID, trace ID, lineage, timestamp, outcome, safe details, redaction/proof references, and schema version. |

### Conformance fixture taxonomy and CI matrix

Every conformance fixture must declare:

- `id`
- `owner_story`
- `requirement_refs`
- `contract_refs`
- `fixture_type`: positive, negative, property, perf, security, release, ui-e2e, accessibility, compatibility
- `surface`: daemon, Rust SDK, TypeScript SDK, CLI, MCP bridge, UI, release
- `profile`: platform/browser/protocol profile where relevant
- `tags`: FR/NFR/AR/UX IDs plus contract tags
- `deterministic_seed` when applicable
- `ci_jobs`

CI must verify fixture coverage by tag for:

- core contracts
- SDK parity
- persistence/recovery
- security/compliance
- MCP compatibility
- UI E2E/accessibility
- performance regression
- release evidence

## Epic List

### Epic 1: First Local Mesh and SDK Bootstrap

A developer can install/run `zornmesh`, start or auto-spawn one trustworthy local daemon, use stable CLI output, and send/receive a first basic envelope through Rust/TypeScript SDK surfaces.

**FRs covered:** FR1, FR2, FR15, FR16, FR17, FR18, FR19, FR20, FR21, FR45, FR46, FR47, FR48, FR61.

**Primary NFR/AR/UX constraints:** custom compileable workspace scaffold, Rust 2024 resolver 3, Bun TypeScript SDK boundary, no external runtime dependencies, single-daemon invariant, local socket trust, stable CLI output, golden fixtures, conformance fixture taxonomy, NO_COLOR/TTY behavior, release naming as `zornmesh`.

### Epic 2: Reliable Agent Coordination

Agents can coordinate beyond the first message using request/reply, pull leases, streaming, ACK/NACK, cancellation, idempotency, durable subscriptions, backpressure, and per-call context.

**FRs covered:** FR3, FR4, FR5, FR6, FR7, FR8, FR9, FR10, FR62.

**Primary NFR/AR/UX constraints:** internal frame/control tier, ACK taxonomy, payload and chunk limits, durable ACK after commit, idempotency window, publisher-visible backpressure, stream quota handling, typed errors, conformance fixtures, property/concurrency tests, performance gates.

### Epic 3: Agent Identity, Capabilities, and Host Bridges

Developers can see who is on the mesh, what each agent can do, safely gate high-privilege capabilities, and bridge existing MCP hosts without modifying those hosts.

**FRs covered:** FR11, FR12, FR13, FR14, FR33, FR34, FR35, FR36, FR39, FR40.

**Primary NFR/AR/UX constraints:** AgentCard A2A 1.0.0 profile, MCP 2025-11-25 bridge fixtures, TypeBox capability schemas, high-privilege default deny, socket permission rejection, secret redaction parity, stable error registry, local-only IPC trust model.

### Epic 4: Forensic Persistence, Trace, and Recovery

Developers can reconstruct, inspect, tail, replay, and recover multi-agent conversations from durable local evidence when something breaks.

**FRs covered:** FR22, FR23, FR24, FR25, FR26, FR27, FR28, FR29, FR30, FR31, FR32.

**Primary NFR/AR/UX constraints:** SQLite/sqlx WAL persistence, writer/reader boundaries, forward-only migrations, audit hash chain, DLQ, retention jobs, trace/correlation indexes, OTel schema, live tail NDJSON, replay safety, disk-full/read-degraded posture, trace fixtures.

### Epic 5: Compliance, Audit, and Release Trust Evidence

Operators and compliance reviewers can verify release integrity, export evidence, prove audit-log integrity, handle redaction/deletion, and map events to required AI-risk/compliance frameworks.

**FRs covered:** FR37, FR38, FR41, FR42, FR43, FR44.

**Primary NFR/AR/UX constraints:** Sigstore signatures, CycloneDX SBOM, `zornmesh inspect sbom`, EU AI Act traceability fields, NIST AI RMF mapping table, GDPR-style redaction while preserving audit integrity, evidence export bundles, offline audit verify, release preflight gates.

### Epic 6: Local Web Control Room and Safe Intervention

Developers can open the local UI, observe connected agents, inspect trace chronology, send direct/broadcast messages safely, confirm outcomes, reconnect/backfill, and copy CLI handoffs.

**FRs covered:** FR49, FR50, FR51, FR52, FR53, FR54, FR55, FR56, FR57, FR58, FR59, FR60.

**Primary NFR/AR/UX constraints:** PRD/UX supersede stale architecture no-GUI statements; React/Bun/Tailwind/Radix-style local UI; loopback-only protected session; bundled/offline assets; no external browser requests; Live Mesh Control Room; Focus Trace Reader; safe composer; broadcast confirmation; per-recipient outcomes; WCAG AA; keyboard/screen-reader support; responsive fallbacks; browser E2E fixtures.

## Cross-Epic Dependency Graph, Gates, and MVP Path

### Cross-epic dependency graph

- Epic 1 gates all later work through buildable workspace, daemon rendezvous, SDK connection, first envelope path, CLI output contracts, fixture taxonomy, and product error registry bootstrap.
- Epic 2 depends on Epic 1 and produces the coordination, ACK, idempotency, streaming, cancellation, durable subscription, and backpressure contracts consumed by Epics 4, 5, and 6.
- Epic 3 depends on Epic 1 plus Story 2.1 / the Epic 2 outcome and error taxonomy, then supplies identity, capability, high-privilege gate, socket-trust, MCP, and redaction contracts consumed by Epics 4, 5, and 6. Story 3.1 and Story 3.5 must not begin before Story 2.1 pins the shared coordination outcome contract.
- Epic 4 depends on Epics 2 and 3 for delivery/identity semantics and supplies persistence, trace, audit, retention-gap, recovery, and inspection evidence consumed by Epics 5 and 6.
- Epic 5 depends on Epics 3 and 4 plus release-engineering evidence from Story 5.1.
- Epic 6 depends on Epics 1-4 and must not begin feature stories until Story 6.1 resolves the UI architecture supersession and shared UI/API taxonomies.

### Release and implementation gates

- **Contract freeze gate:** Stories 1.4, 2.1, 2.4, 3.1, 3.2, 3.5, 4.1, and 4.9 pin shared contracts before downstream stories claim implementation.
- **SDK parity gate:** Every Epic 2 coordination fixture must declare Rust SDK and TypeScript SDK expected behavior; if TypeScript implementation lags, the story must record an explicit unsupported/parity-gap result rather than silently passing Rust-only behavior as v0.1 parity.
- **Persistence gate:** Story 4.1 must pass before durable ACK, replay, audit export, retention, or UI timeline completeness can be claimed.
- **Compliance evidence gate:** Stories 5.1-5.3 must pass before v0.1 release trust evidence is considered complete.
- **UI gate:** Story 6.1 must pass before Stories 6.2-6.9.
- **Performance gate:** NFR-P1 through NFR-P9, NFR-R3, and NFR-CA3 must be benchmarked before v0.1 release readiness.

### Non-destructive MVP path / implementation sequence

These markers prioritize implementation without removing approved stories.

- **MVP-P0 thin local mesh:** Stories 1.1, 1.2, 1.3, 1.4, 1.6, 2.1, 3.1, 4.1.
- **MVP-P1 reliable coordination:** Stories 1.5, 1.7, 2.2-2.8, 3.2-3.5, 4.2-4.4.
- **MVP-P2 recovery/compliance core:** Stories 3.6-3.8, 4.5-4.10, 5.1-5.5.
- **MVP-P3 local UI:** Stories 6.1-6.9 after UI gate.

## Epic 1: First Local Mesh and SDK Bootstrap

A developer can install/run `zornmesh`, start or auto-spawn one trustworthy local daemon, use stable CLI output, and send/receive a first basic envelope through Rust/TypeScript SDK surfaces.

### Story 1.1: Create Buildable Workspace and Command Skeleton

As a developer,
I want a buildable `zornmesh` workspace with the correct Rust/Bun boundaries and command skeleton,
So that implementation can proceed from a verified scaffold instead of drifting into unsupported tooling or names.

**FR traceability:** Supported: FR61; Gated: FR45, FR48.

**Additional traceability:** AR1-AR11, AR24, AR25; NFR-M2, NFR-M3, NFR-M4, NFR-M5; owns fixture taxonomy and CI matrix bootstrap.

**Acceptance Criteria:**

**Given** a fresh checkout of the repository
**When** the developer runs the documented build/check command
**Then** the root Rust workspace compiles with Rust 2024 resolver 3 and includes the planned crate boundaries for core, proto, store, broker, rpc, SDK, daemon, CLI, and xtask
**And** no alternate Rust async runtime or unsupported external broker dependency is introduced.

**Given** the TypeScript SDK boundary is initialized
**When** the developer runs the documented Bun test command in `sdks/typescript`
**Then** a minimal Bun-managed TypeScript SDK package exists and its minimal test passes
**And** no npm, pnpm, yarn, Node-specific runner, or fake Python v0.1 package is added.

**Given** the `zornmesh` CLI skeleton exists
**When** the developer runs `zornmesh --help`
**Then** the command prints stable help output using the public `zornmesh` name
**And** initial CLI golden-output fixtures exist for root help and trace help.

**Given** the conformance-first architecture requirement
**When** the scaffold is complete
**Then** `conformance/`, `fixtures/cli/`, `fixtures/errors/`, and `test-infra/` exist with README or manifest files explaining their fixture ownership
**And** the scaffold includes a thin smoke path such as a core envelope/domain round-trip or CLI/help fixture check.

**Given** a dev agent starts from this story
**When** they review the repository after implementation
**Then** `just check`, `just test`, `just lint`, `just docs`, and `just conformance` exist as human entrypoints
**And** generation, fixture, conformance, and release-preflight work delegates to explicit `cargo xtask <subcommand>` entrypoints.
**And** missing required tooling fails explicitly rather than silently succeeding.

**Given** scaffold scope can drift into premature implementation
**When** this story is complete
**Then** it is limited to scaffold, fixtures, and smoke validation
**And** daemon routing, durable store, and auto-spawn semantics remain out of scope except for explicit compileable boundaries.

### Story 1.2: Establish Local Daemon Rendezvous and Trust Checks

As an operator,
I want the local daemon to start, own one trusted socket, reject unsafe privilege/permission states, and report its lifecycle state,
So that every SDK and CLI interaction has one trustworthy local mesh endpoint.

**FR traceability:** Implemented: FR17, FR18, FR19, FR20.

**Additional traceability:** NFR-P1, NFR-S1, NFR-S2, NFR-S3, NFR-R1, NFR-R4; AR22, AR23.

**Acceptance Criteria:**

**Given** no daemon is currently running for the user
**When** the operator starts `zornmesh daemon`
**Then** exactly one daemon process owns the resolved local socket path
**And** daemon startup emits a parseable readiness signal or readiness line suitable for SDK and CLI clients.

**Given** a daemon is already running for the user
**When** a second daemon start is attempted or concurrent starts race
**Then** the active daemon remains the sole socket owner
**And** the losing process exits with a stable, documented error explaining the existing owner.

**Given** the resolved socket file exists but the owning daemon process is gone after a crash
**When** daemon startup or SDK/CLI connection validation runs
**Then** stale ownership is detected and the socket is safely removed or quarantined according to documented rules
**And** no second daemon starts against an unverified or cross-user socket.

**Given** the daemon is launched with elevated privileges or an unsafe socket ownership/permission state
**When** startup or client connection validation runs
**Then** the daemon or client rejects the operation with a named error
**And** the error message explains the local-only trust requirement without leaking sensitive paths beyond safe diagnostics.

**Given** the operator disables auto-spawn through the configured environment variable
**When** an SDK or CLI client attempts to connect while no daemon is running
**Then** no daemon is spawned
**And** the client returns a stable daemon-unreachable error with a remediation hint.

**Given** the daemon receives SIGTERM
**When** shutdown begins
**Then** the daemon stops accepting new connections and reports a draining state
**And** the process exits according to the configured shutdown budget with a documented exit outcome.

**Given** the shutdown budget expires while work remains in flight
**When** the daemon exits or escalates shutdown
**Then** over-budget work is marked with a structured shutdown-budget-exceeded outcome or dead-letter reason where durable state exists
**And** the process exits with a documented status instead of silently losing work.

### Story 1.3: Connect Rust SDK to Auto-Spawned Daemon

As an adopter building a Rust agent,
I want `Mesh.connect()` to find or auto-spawn the local daemon and complete a readiness handshake,
So that my agent can join the local mesh without manual daemon setup.

**FR traceability:** Implemented: FR15, FR16, FR18, FR61.

**Additional traceability:** NFR-P1, NFR-R1; owns SDK connect-budget and auto-spawn opt-out behavior.

**Acceptance Criteria:**

**Given** no daemon is running and auto-spawn is enabled
**When** a Rust agent calls the SDK connect entrypoint
**Then** the SDK resolves the local socket path, starts the daemon, retries connection until readiness or timeout, and returns a connected client
**And** the operation completes within NFR-P1: <= 200 ms p95 from SDK connect invocation to daemon readiness on the v0.1 reference platform.

**Given** a daemon is already ready
**When** the Rust SDK connect entrypoint is called
**Then** the SDK connects to the existing socket without spawning another daemon
**And** the daemon ownership state remains unchanged.

**Given** auto-spawn is disabled
**When** the Rust SDK connect entrypoint is called while no daemon is running
**Then** the SDK does not spawn a daemon
**And** it returns a stable typed error that CLI and future TypeScript SDK surfaces can map consistently.

**Given** the daemon starts but does not become ready before the connect budget expires
**When** the SDK connect retry loop completes
**Then** the SDK returns a timeout/retryable typed error
**And** diagnostic details include safe daemon state and remediation, not raw secrets or unstable debug strings.

**Given** the SDK connect contract is implemented
**When** the integration test runs concurrent Rust SDK connect attempts
**Then** all successful clients connect to the same daemon instance
**And** failed clients expose typed errors without orphaning daemon processes.

**Given** shared SDK connect fixtures are created
**When** no daemon, existing daemon, disabled auto-spawn, stale socket, readiness timeout, and concurrent connect scenarios execute
**Then** Rust and TypeScript SDKs consume the same state names, error codes, and timeout expectations
**And** future SDK parity cannot silently diverge from the connect contract.

### Story 1.4: Send First Local Publish/Subscribe Envelope

As a developer,
I want two local agents to publish and subscribe to a first envelope through the daemon,
So that I can prove the local mesh coordinates real agent traffic end-to-end.

**FR traceability:** Implemented: FR1, FR2, FR61.

**Additional traceability:** NFR-P2, NFR-P3, NFR-S7, NFR-SC2, NFR-SC3, NFR-SC4; AR12, AR24; owns canonical envelope schema and subject grammar baselines.

**Acceptance Criteria:**

**Given** a ready daemon and two Rust SDK clients connected through the local socket
**When** one client subscribes to a subject and the other publishes an envelope to that subject
**Then** the subscriber receives one delivery attempt in the no-retry happy path under at-least-once semantics
**And** the received envelope includes stable source agent reference, subject, timestamp, correlation ID, and payload metadata.

**Given** a subscriber registers an exact subject, a prefix pattern, or a hierarchical wildcard pattern
**When** matching and non-matching envelopes are published
**Then** only matching envelopes are delivered
**And** pattern behavior is covered by shared conformance fixtures rather than one-off test data.

**Given** a subject or subject pattern exceeds 256 bytes, exceeds 8 levels, uses reserved prefixes incorrectly, contains invalid wildcard syntax, exceeds 256 subscribers for one subject pattern, or exceeds 4,096 total subscriptions
**When** publish or subscribe validation runs
**Then** the operation is rejected with a stable subject-validation or subscription-cap error
**And** no invalid route, subscription, or retained fixture state is created.

**Given** an inbound envelope is malformed, missing required metadata, or exceeds the initial configured size limit
**When** the daemon receives it
**Then** the daemon rejects it with a stable typed error
**And** no subscriber receives the invalid envelope.

**Given** a publisher sends an envelope through the Rust SDK
**When** the daemon accepts it for routing
**Then** the publisher receives an explicit send result
**And** the result distinguishes accepted, rejected, daemon-unreachable, and validation-failed outcomes.

**Given** the first-message smoke path is implemented
**When** a dev agent runs the documented integration test
**Then** the test starts the daemon, connects two SDK clients, sends an envelope, receives it, and tears down without orphaned processes
**And** the same fixture can be reused by future TypeScript SDK parity work without asserting exactly-once delivery under retry, reconnect, or failure.

### Story 1.5: Add TypeScript SDK Bootstrap Parity

As an adopter building a TypeScript agent,
I want the Bun-managed TypeScript SDK to connect to the local daemon and pass the same first-message fixture as Rust,
So that v0.1 supports cross-language agent construction from the start.

**FR traceability:** Implemented: FR61; Verified: FR1, FR2; Supported: FR15, FR16, FR18.

**Acceptance Criteria:**

**Given** the TypeScript SDK package exists under `sdks/typescript`
**When** the developer runs the documented Bun install/test command
**Then** the package uses Bun-managed dependencies and Bun tests
**And** no npm, pnpm, yarn, Vitest, Jest, Mocha, or Node-specific runtime assumption is introduced.

**Given** a ready daemon from the shared test harness
**When** a TypeScript SDK client connects
**Then** it completes the same readiness/connect contract as the Rust SDK for the supported v0.1 path
**And** connection failures expose stable error fields equivalent to Rust SDK error semantics.

**Given** the shared first-message conformance fixture
**When** a TypeScript SDK client publishes and subscribes through the daemon
**Then** the same subject, envelope metadata, and delivery result expectations pass as the Rust fixture
**And** serialization names at the wire boundary remain snake_case.

**Given** no daemon is running, auto-spawn is disabled, a stale socket exists, startup times out, or concurrent TypeScript connects race
**When** the TypeScript SDK connects during this story
**Then** TypeScript follows the Rust shared auto-spawn policy and state taxonomy from Story 1.3
**And** enabled, disabled, stale-socket, timeout, and concurrent outcomes match the shared SDK connect fixtures.

**Given** SDK documentation is generated or checked for public APIs
**When** the TypeScript SDK bootstrap is complete
**Then** connect, publish, and subscribe entrypoints have minimal public documentation and usage examples
**And** the docs use `zornmesh` naming consistently.

### Story 1.6: Stabilize CLI Read Outputs and Exit Contracts

As a developer or operator,
I want every CLI subcommand to have stable human, JSON, stderr, non-interactive, and exit-code behavior,
So that I can use `zornmesh` interactively and in scripts without fragile parsing.

**FR traceability:** Implemented: FR45, FR46, FR48.

**Additional traceability:** NFR-C5; AR24, AR25; pins product error registry, CLI JSON/NDJSON envelope, stderr, and exit-code mappings.

**Acceptance Criteria:**

**Given** an initial read subcommand such as help, daemon status, agents, or doctor
**When** the command succeeds in human mode
**Then** success output is written to stdout with stable wording for fixture-covered fields
**And** warnings, if any, are distinguishable from primary success output.

**Given** a supported read subcommand is invoked with `--output json`
**When** the command succeeds
**Then** stdout contains only valid JSON with the documented top-level shape
**And** no human prose, ANSI codes, or diagnostics are mixed into stdout.

**Given** a streaming read mode is introduced or fixture-scaffolded
**When** JSON output is requested for streaming records
**Then** records are emitted as NDJSON with one event per line
**And** each event includes schema version, event type, sequence, and data fields.

**Given** a command would normally prompt for input
**When** it is invoked with the non-interactive fail-fast flag
**Then** the command fails without prompting
**And** stderr contains a stable error code and remediation text.

**Given** the command runs in a non-TTY context or with `NO_COLOR=1`
**When** human output is produced
**Then** ANSI color is disabled
**And** JSON output remains byte-identical regardless of TTY state.

**Given** a command fails due user error, daemon-unreachable, validation failure, permission denied, or not-found
**When** the failure is returned
**Then** the CLI exits with a stable documented exit code
**And** stderr includes the registered product error code without leaking secrets.

**Given** any current or future subcommand is added after this story
**When** global CLI contract tests run
**Then** the subcommand honors global flag parsing, stdout/stderr separation, non-interactive fail-fast, `NO_COLOR`/TTY behavior, and stable exit-code mapping
**And** unsupported output formats fail with typed errors rather than falling back silently.

**Given** the same setting is provided by defaults, config file, environment variable, and CLI flag
**When** effective configuration is resolved for any subcommand
**Then** precedence is deterministic and fixture-covered as defaults < config file < environment < CLI flag
**And** human, JSON, and error outputs report the effective value source only when safe.

### Story 1.7: Provide Doctor, Shutdown, and Shell Completion Basics

As an operator,
I want first-day diagnostics, graceful shutdown behavior, and shell completions,
So that I can understand and operate the local mesh without inspecting runtime files by hand.

**FR traceability:** Implemented: FR17, FR21, FR47, FR48.

**Additional traceability:** NFR-O4, NFR-S5; AR11, AR25, AR28; signature/SBOM visibility reports unavailable until release evidence exists.

**Acceptance Criteria:**

**Given** a healthy local daemon
**When** the operator runs `zornmesh doctor`
**Then** the command reports daemon status, version, socket path/ownership, schema version, OTel reachability, signature verification status, SBOM identity/status, and local trust posture
**And** the same information is available in JSON mode.

**Given** signature, SBOM, OTel, schema, or trust evidence is missing, unverifiable, or unavailable for the current build
**When** the operator runs `zornmesh doctor`
**Then** each missing evidence source is reported as degraded, unavailable, or unverifiable with a stable status
**And** no required diagnostic category is omitted because evidence is not yet produced.

**Given** the daemon is unreachable, unhealthy, draining, or blocked by unsafe socket permissions
**When** the operator runs `zornmesh doctor`
**Then** the command returns a stable status and remediation hint
**And** it does not require the operator to know or inspect internal runtime directories.

**Given** the operator requests daemon shutdown
**When** shutdown is initiated from CLI or signal handling
**Then** the daemon reports draining, stops accepting new work, honors the configured shutdown budget, and exits with a documented outcome
**And** any uncompleted in-flight work is surfaced through a stable diagnostic status.

**Given** a supported shell is requested
**When** the developer runs the shell-completion command
**Then** the CLI emits valid completions for that shell
**And** the generated completions include the initial daemon, doctor, agents, help, and output-mode flags.

**Given** an unsupported shell is requested
**When** the developer runs the shell-completion command
**Then** the CLI returns `E_UNSUPPORTED_SHELL` with the supported-shell list
**And** no partial completion script is written to stdout.

**Given** first-day operator workflows are fixture-covered
**When** CLI golden tests run
**Then** doctor healthy JSON, doctor daemon-unreachable output, daemon help, and completion generation fixtures pass
**And** output remains stable across TTY and non-TTY execution.

## Epic 2: Reliable Agent Coordination

Agents can coordinate beyond the first message using request/reply, pull leases, streaming, ACK/NACK, cancellation, idempotency, durable subscriptions, backpressure, and per-call context.

**Durability contract:** Stories 2.1-2.8 may claim durable ACK, lease, idempotency, subscription, retry, or backpressure state only after the relevant SQLite/sqlx commit succeeds. In-memory-only state must return typed persistence-unavailable or unsupported outcomes and must never claim durable success.

### Story 2.1: Establish Coordination Result and ACK/NACK Contract

As an agent author,
I want every send-side and receive-side operation to return stable coordination outcomes,
So that my agent can distinguish accepted, rejected, durable, delivered, retryable, and terminal failure states without parsing logs.

**FR traceability:** Implemented: FR6, FR62.

**Additional traceability:** AR13, AR14, AR24; pins ACK-layer taxonomy, coordination outcome schema, product error categories, and delivery-state vocabulary.

**Acceptance Criteria:**

**Given** an agent sends an envelope through the SDK
**When** the daemon syntactically accepts the frame for processing
**Then** the SDK can observe a transport-level accepted outcome
**And** that outcome is distinct from durable persistence and consumer delivery outcomes.

**Given** a sent envelope is durably accepted by the broker/store path available in this story
**When** the relevant state is committed or recorded according to the current persistence contract
**Then** the SDK can observe a durable-accepted outcome
**And** retries do not treat transport acceptance alone as durable success.

**Given** a consumer processes or rejects a delivered envelope
**When** it returns ACK or NACK
**Then** the broker records delivery outcome using stable accepted, acknowledged, rejected, failed, timed-out, retryable, or terminal categories
**And** NACK results include a safe structured reason category.

**Given** a coordination operation fails validation, authorization, daemon reachability, timeout, or payload limit checks
**When** the SDK returns an error
**Then** the error exposes stable code, category, retryable flag, and safe details
**And** equivalent semantics are available through the versioned envelope/error contract shared by the Rust SDK, TypeScript SDK, CLI, and daemon.

**Given** ACK/NACK behavior is implemented
**When** conformance tests exercise transport ACK, durable ACK, delivery ACK, and NACK paths
**Then** each outcome is fixture-covered and observable through SDK results or structured daemon events
**And** no outcome requires string-matching human log output.

**Given** the coordination contract is versioned
**When** envelope and error fixtures are created
**Then** the canonical envelope schema, internal frame definitions, delivery-state taxonomy, and product error registry are pinned by explicit versions under the `zornmesh-core` and `zornmesh-proto` contract boundaries
**And** breaking changes require migration notes, compatibility fixtures, and an explicit release-process decision.

**Given** a wire frame has an invalid length, unknown frame type, truncated payload, malformed payload encoding, or unsupported schema version
**When** the daemon parses the frame
**Then** parsing fails before unbounded allocation or state mutation
**And** the connection receives a stable protocol error or close reason covered by negative fixtures.

### Story 2.2: Send Correlated Request/Reply with Timeout

As an agent author,
I want one agent to request work from another and receive one correlated reply or typed timeout,
So that agents can coordinate task handoffs without ad-hoc files, ports, or polling.

**FR traceability:** Implemented: FR3, FR62.

**Additional traceability:** NFR-P3; depends on Story 2.1 outcome taxonomy and the shared tracecontext contract once introduced.

**Acceptance Criteria:**

**Given** two connected agents and a registered request target
**When** agent A sends a request to agent B with a correlation ID and timeout
**Then** agent B receives the request with source, target, subject/method, correlation ID, trace context, and payload metadata
**And** agent A receives exactly one correlated reply when B responds before timeout.

**Given** a request target does not reply before the configured timeout
**When** the timeout elapses
**Then** agent A receives a typed timeout result
**And** the daemon does not later deliver a stale reply as a successful response for the completed request.

**Given** a request target rejects the request or returns a structured failure
**When** the reply path completes
**Then** agent A receives a typed rejected/failed result with safe details
**And** retryability is represented by the shared coordination outcome contract from Story 2.1.

**Given** a request target sends multiple replies before timeout
**When** replies are accepted or persisted
**Then** the first terminal reply by daemon sequence wins
**And** later replies are recorded as duplicate or late events and never reach the requester as separate successes.

**Given** two requests are in flight concurrently between the same agents
**When** replies arrive in reverse order
**Then** each reply is matched to the correct request by correlation ID
**And** no response is delivered to the wrong caller.

**Given** request/reply conformance tests run
**When** happy path, timeout, rejected reply, out-of-order reply, daemon disconnect, and concurrent-client scenarios execute
**Then** all scenarios pass for the Rust SDK path with documented timeout bounds and use fixtures that future TypeScript parity tests can consume.

**Given** request/reply benchmark tests run
**When** loopback requests with <= 4 KiB payloads execute with persistence enabled
**Then** p50 latency is <= 2 ms and p99 latency is <= 20 ms on the v0.1 reference platform.

### Story 2.3: Fetch, Lease, ACK, and NACK Pulled Envelopes

As an agent author,
I want consumers to fetch work with explicit leases and acknowledge or reject each envelope,
So that agents can process work safely without losing or duplicating delivery state invisibly.

**FR traceability:** Implemented: FR4, FR6.

**Acceptance Criteria:**

**Given** envelopes are available for a pull-based consumer
**When** the consumer calls fetch with a batch size and lease duration
**Then** the daemon returns only envelopes assigned to that consumer with lease IDs and expiry metadata
**And** fetched envelopes are not simultaneously leased to another consumer.

**Given** a consumer calls fetch with zero, negative, over-maximum, or unsupported batch size or lease duration
**When** fetch validation runs
**Then** the daemon returns a stable fetch-validation error
**And** no lease, delivery attempt, or cursor state is created.

**Given** a consumer successfully processes a leased envelope
**When** it sends ACK for the lease
**Then** the broker records delivery acknowledgement
**And** the envelope is not returned by subsequent fetch calls for the same subscription.

**Given** a consumer cannot process a leased envelope
**When** it sends NACK with a safe structured reason
**Then** the broker records the failure category and makes the envelope eligible for retry, backoff, or terminal handling according to policy
**And** the NACK result maps to the shared outcome contract from Story 2.1.

**Given** ACK or NACK references an expired, unknown, already-acknowledged, already-nacked, or foreign lease
**When** the daemon evaluates the acknowledgement
**Then** it returns a stable lease-not-owned, lease-expired, or already-terminal result
**And** no unrelated delivery state is mutated.

**Given** a consumer is still processing before lease expiry
**When** it renews the lease within the allowed window
**Then** the lease expiry is extended without duplicating delivery
**And** renewal failures return stable typed errors.

**Given** a lease expires without ACK, NACK, or renewal
**When** another fetch is issued after expiry
**Then** the envelope can be leased again according to at-least-once delivery semantics
**And** retry attempts are visible through structured event/audit metadata for downstream trace work.

### Story 2.4: Add Idempotency Keys and Retry-Safe Sends

As an agent author,
I want send operations to carry idempotency keys and preserve per-call context,
So that retries do not create duplicate work or lose traceability.

**FR traceability:** Implemented: FR8, FR62.

**Additional traceability:** NFR-R2; AR19; pins idempotency scope, default 24h dedupe window, persisted retry state, and expired-window behavior.

**Acceptance Criteria:**

**Given** an agent sends an envelope with an idempotency key
**When** the same sender retries the same operation within the configured deduplication window
**Then** the broker returns the original accepted outcome instead of creating duplicate routed work
**And** the response makes clear that the result came from deduplication.

**Given** the same sender reuses an idempotency key with a different subject, payload fingerprint, operation kind, or semantic request shape
**When** deduplication validation runs
**Then** the daemon returns a stable idempotency-conflict error
**And** no new routed work is created for the conflicting operation.

**Given** two different senders use the same idempotency key
**When** both send operations are valid
**Then** deduplication scope prevents cross-agent collision
**And** each sender's operation is evaluated independently.

**Given** a retry occurs after a transport failure but before durable outcome is known
**When** the sender retries with the same idempotency key
**Then** the daemon resolves the retry to a stable accepted/rejected/unknown outcome according to stored coordination state
**And** the SDK does not fabricate success when durability is unknown.

**Given** the daemon restarts within the idempotency deduplication window
**When** a sender retries with the same idempotency key after restart
**Then** persisted idempotency records resolve the retry consistently with pre-restart behavior
**And** in-memory cache loss cannot duplicate routed work inside the active window.

**Given** a send operation carries timeout and trace context
**When** the operation is retried or deduplicated
**Then** correlation ID, trace context, timeout/deadline, and source agent reference remain stable across attempts
**And** downstream trace/audit work can distinguish first attempt from retry attempt.

**Given** idempotency conformance tests run
**When** duplicate, collision, different-payload conflict, daemon-restart, expired-window, and retry-after-transport-failure scenarios execute
**Then** no duplicate delivery occurs inside the deduplication window
**And** expired-window behavior is explicit and fixture-covered.

### Story 2.5: Stream Response Chunks with Byte-Budget Flow Control

As an agent author,
I want agents to stream multi-part responses through bounded chunks,
So that large or incremental outputs can move through the mesh without unbounded buffering.

**FR traceability:** Implemented: FR5.

**Additional traceability:** NFR-P4, NFR-SC4; AR12, AR13; pins stream frame and byte-budget flow-control contract.

**Acceptance Criteria:**

**Given** an agent starts a streaming response for a correlated request
**When** it sends multiple stream chunks
**Then** each chunk carries stream identity, correlation ID, sequence/order metadata, finality status, and size metadata
**And** the receiving agent can reconstruct chunks in order or detect a gap.

**Given** a stream chunk exceeds the configured chunk size limit
**When** the daemon receives it
**Then** the chunk is rejected with a stable payload-limit error
**And** the stream is not silently truncated or delivered partially as success.

**Given** a stream reaches the configured byte-budget window
**When** the sender attempts to continue without receiver progress or available budget
**Then** flow control prevents unbounded buffering
**And** the sender receives a typed backpressure or quota result.

**Given** a stream completes normally
**When** the final chunk is accepted
**Then** the receiver observes a terminal complete state
**And** the sender receives a terminal send outcome consistent with the coordination contract.

**Given** stream benchmark tests run
**When** one stream uses the configured 256 KiB byte-budget window on the v0.1 reference platform
**Then** sustained stream throughput is >= 50 MiB/sec
**And** benchmark fixtures fail if throughput regresses below the gate.

**Given** a stream is interrupted by daemon disconnect, invalid chunk order, or receiver failure
**When** the stream cannot complete
**Then** both sender and receiver can observe a terminal failed/cancelled state
**And** conformance fixtures cover complete, oversize, gap, quota, and interrupted stream scenarios.

### Story 2.6: Cancel In-Flight Requests and Streams

As an agent author,
I want to cancel an in-flight request or stream by correlation ID,
So that agents can stop work that is no longer needed and observe a clear terminal outcome.

**FR traceability:** Implemented: FR7.

**Acceptance Criteria:**

**Given** a request is in flight and has not completed
**When** the caller cancels by correlation ID
**Then** the daemon records the request as cancelled
**And** the requester receives a terminal cancelled outcome rather than a timeout or success.

**Given** a target agent is processing a cancellable request
**When** cancellation is accepted
**Then** the target receives a cancellation signal or cancellation-visible delivery state
**And** any later reply from the target is rejected or recorded as late according to the coordination contract.

**Given** a reply and cancellation race to become terminal for the same correlation ID
**When** both events are evaluated or persisted
**Then** the first durably committed terminal event by daemon sequence wins
**And** the losing event is recorded as late or already-complete deterministically across replay.

**Given** a stream is in progress
**When** the requester cancels the stream by correlation ID
**Then** the sender and receiver observe a terminal cancelled state
**And** no further chunks are accepted as successful for that stream.

**Given** cancellation is requested for an unknown, completed, or expired correlation ID
**When** the daemon evaluates the request
**Then** it returns a stable not-found, already-complete, or expired result
**And** the operation does not mutate unrelated in-flight work.

**Given** cancellation conformance tests run
**When** request cancellation, stream cancellation, late reply, unknown ID, and already-complete scenarios execute
**Then** terminal states are deterministic and observable through SDK results and structured daemon events.

### Story 2.7: Resume Durable Subscriptions After Daemon Restart

As an agent author,
I want a durable subscription to resume from its last acknowledged position after daemon restart,
So that my agent can continue processing without manually tracking offsets.

**FR traceability:** Implemented: FR9.

**Acceptance Criteria:**

**Given** an agent creates a durable subscription with a stable subscription identity
**When** envelopes are delivered and acknowledged
**Then** the daemon records durable subscription identity, scope, last acknowledged sequence, lease state, retry counters, and retention-gap markers through SQLite/sqlx
**And** the recorded position is persisted through the SQLite/sqlx durability model rather than in-memory state and is isolated from other subscriptions.

**Given** the daemon restarts after acknowledged and unacknowledged deliveries
**When** the agent reconnects with the same durable subscription identity
**Then** acknowledged envelopes are not redelivered
**And** unacknowledged or expired-lease envelopes become eligible for redelivery according to at-least-once semantics.

**Given** a durable subscription identity is missing, duplicated incorrectly, or conflicts with an existing subscription scope
**When** the subscription is created or resumed
**Then** the daemon returns a stable validation/conflict error
**And** no subscription state is silently overwritten.

**Given** retention policy removes data required by a durable subscription
**When** the subscription attempts to resume before or after the retained range
**Then** the daemon reports a structured retention-gap condition
**And** the agent receives remediation guidance rather than an empty success.

**Given** durable subscription conformance tests run
**When** restart with cleared memory caches, crash, reconnect, acknowledged, unacknowledged, conflicting identity, corrupt durable state, and retention-gap scenarios execute
**Then** resume behavior is deterministic and leaves observable state for downstream trace/recovery stories.

### Story 2.8: Surface Backpressure at Queue and Lease Bounds

As an agent author,
I want publishers to receive clear backpressure feedback when consumers fall behind,
So that agents can slow down, retry later, or route work safely instead of losing messages silently.

**FR traceability:** Implemented: FR10.

**Additional traceability:** NFR-P2, NFR-P5, NFR-P6, NFR-SC2; depends on Story 2.1 delivery-state taxonomy and the persistence gate before durable outcomes are claimed.

**Acceptance Criteria:**

**Given** a subscription reaches its configured queue bound
**When** a publisher sends additional matching envelopes
**Then** the broker surfaces a publisher-visible backpressure outcome within 100 ms
**And** no envelope is silently dropped unless an explicit policy says so and the result reports that policy.

**Given** a consumer repeatedly misses acknowledgement or lease budgets
**When** the broker evaluates delivery health
**Then** the consumer or subscription is marked backpressured, retrying, or failed according to stable state names
**And** those states are visible through SDK results and diagnostic events.

**Given** a publisher receives a backpressure outcome
**When** it inspects the result
**Then** the result includes safe details: subject/subscription scope, queue bound, lease/ACK threshold, exceeded limit, retryability, suggested delay or remediation category, and whether the send was accepted, rejected, or deferred
**And** it does not expose secret payload data.

**Given** routing and memory benchmark tests run under nominal load
**When** sustained publish/subscribe traffic executes with persistence enabled
**Then** throughput is >= 5,000 envelopes/sec and daemon resident memory remains <= 256 MiB on the v0.1 reference platform
**And** benchmark fixtures fail on regressions beyond the documented tolerance.

**Given** backpressure clears after the consumer catches up or bounds are increased
**When** new matching envelopes are published
**Then** delivery resumes without requiring daemon restart
**And** the state transition is observable for downstream trace/UI surfaces.

**Given** backpressure conformance tests run
**When** queue-bound, lease-missed, recovery, explicit-drop-policy, and publisher-timeout scenarios execute
**Then** outcomes are deterministic, fixture-covered, and compatible with the shared delivery-state taxonomy.

## Epic 3: Agent Identity, Capabilities, and Host Bridges

Developers can see who is on the mesh, what each agent can do, safely gate high-privilege capabilities, and bridge existing MCP hosts without modifying those hosts.

### Story 3.1: Register Minimal AgentCard Identity

As an agent author,
I want my agent to register a stable AgentCard-compatible identity with the local mesh,
So that routing, trace, audit, and future UI surfaces can consistently identify who sent and received work.

**FR traceability:** Implemented: FR11, FR14.

**Additional traceability:** AR16, AR19; pins AgentCard raw/canonical required fields and identity version.

**Depends on:** Epic 1 scaffold and Story 2.1 error/outcome contract.

**Acceptance Criteria:**

**Given** an agent connects to the daemon
**When** it submits minimal AgentCard-compatible identity metadata
**Then** the daemon validates required identity fields, version, display name or stable ID, and source/transport metadata
**And** rejects malformed or unsupported identity payloads with stable typed errors.

**Given** an identity is accepted
**When** the agent publishes, subscribes, requests, replies, fetches, ACKs, or NACKs
**Then** the operation is associated with the stable agent reference
**And** downstream trace, audit, delivery, and UI contracts can rely on that reference.

**Given** an older or non-canonical identity shape is accepted for compatibility
**When** normalization occurs
**Then** the daemon stores both raw input and canonical normalized form for audit/debugging
**And** the canonical form is used for routing and API output.

**Given** duplicate identity registration occurs within the same local trust boundary
**When** the duplicate is compatible with the existing identity
**Then** the daemon resolves it deterministically to the same canonical agent reference
**And** incompatible duplicates return a stable conflict error.

**Given** the AgentCard-compatible identity contract is introduced
**When** fixtures are created
**Then** the supported AgentCard profile version, required fields, canonical normalization rules, and stable error codes are pinned in the central conformance manifest
**And** daemon, CLI, Rust SDK, TypeScript SDK, JSON-RPC, and fixture expectations consume the same contract source.

**Given** identity conformance tests run
**When** canonical, missing-required-field, unsupported-version, duplicate-compatible, and duplicate-conflict fixtures execute
**Then** registration behavior is deterministic and fixture-covered.

### Story 3.2: Advertise and Resolve Symmetric Capabilities

As an adopter,
I want agents to advertise both offered and consumed capabilities,
So that I can understand what each agent can do and how agents may interact.

**FR traceability:** Implemented: FR12, FR14.

**Additional traceability:** AR17; pins capability schema ownership across TypeBox, Protobuf, and JSON-RPC.

**Depends on:** Story 3.1.

**Acceptance Criteria:**

**Given** an agent registers identity metadata
**When** it submits offered and consumed capability descriptors
**Then** the daemon validates capability IDs, versions, schema references, and declared direction
**And** invalid or unsupported capability descriptors are rejected with stable typed errors.

**Given** capabilities are accepted
**When** an adopter lists agents or inspects one agent
**Then** the output includes the agent identity, offered capabilities, consumed capabilities, versions, and safe summaries
**And** the output is available through SDK and CLI structured formats.

**Given** capability schemas are provided for capability input/output contracts
**When** the daemon stores or resolves the capability
**Then** schema metadata is associated with the capability without generating the canonical internal envelope model from it
**And** TypeBox/JSON Schema ownership remains distinct from Protobuf internal model ownership.

**Given** an agent updates its capability declaration during a session
**When** the update is valid
**Then** the registry updates deterministically and emits an observable capability-change event
**And** invalid updates do not partially mutate the registry.

**Given** capability schema support is introduced
**When** capability fixtures are created
**Then** the supported schema dialect, allowed annotation subset, secret-field annotation form, versioning rules, and unsupported-schema errors are pinned in the central capability fixture manifest
**And** schema fixtures run in the daemon, Rust SDK, TypeScript SDK, CLI, and bridge CI matrix.

**Given** capability resolution conformance tests run
**When** offered-only, consumed-only, both-directions, invalid-schema-reference, and update scenarios execute
**Then** resolved capability output is deterministic across Rust SDK, TypeScript SDK, CLI, and fixture expectations.

### Story 3.3: Gate High-Privilege Capabilities by Local Allowlist

As an operator,
I want high-privilege capabilities to be denied unless explicitly allow-listed,
So that one local agent cannot silently advertise or invoke dangerous actions.

**FR traceability:** Implemented: FR13, FR39.

**Depends on:** Stories 3.1, 3.2.

**Acceptance Criteria:**

**Given** a capability is marked high-privilege by local policy
**When** an agent attempts to advertise it without an allowlist entry
**Then** registration rejects that capability with a stable authorization error
**And** the agent may still register non-rejected capabilities when safe to do so.

**Given** an agent is not allow-listed for a high-privilege capability
**When** it attempts to invoke or consume that capability
**Then** the daemon rejects the invocation before dispatch
**And** no downstream agent receives the unauthorized request.

**Given** an operator provides a valid local allowlist entry
**When** the allow-listed agent advertises or invokes the high-privilege capability
**Then** the operation is permitted according to the local policy
**And** the decision is recorded as an observable authorization event.

**Given** the allowlist file or config is missing, malformed, or unreadable
**When** high-privilege policy evaluation runs
**Then** default policy is deny
**And** diagnostics explain the safe remediation path without exposing secrets.

**Given** an allowlist entry is revoked while an agent session is active
**When** policy reload or policy re-evaluation observes the revocation
**Then** the revoked capability is removed or marked unavailable for that agent before any new dispatch
**And** queued or undispatched invocations are denied with a stable authorization error while already-dispatched work follows the documented in-flight policy and emits an audit event.

**Given** authorization conformance tests run
**When** deny-by-default, allow-listed advertise, allow-listed invoke, malformed-policy, active-session-revocation, and unauthorized-dispatch scenarios execute
**Then** no high-privilege capability bypasses policy and every denial has a stable error code.

### Story 3.4: Enforce Local Socket Permission Model on Agent Connections

As an operator,
I want the mesh to reject agent connections that do not satisfy the local socket trust model,
So that cross-user or unsafe local processes cannot join the bus by accident.

**FR traceability:** Implemented: FR40.

**Depends on:** Epic 1 local daemon/socket trust baseline and Story 3.1.

**Acceptance Criteria:**

**Given** an agent connects over the local IPC transport
**When** the daemon evaluates peer credentials and socket ownership
**Then** the connection is accepted only when it matches the invoking user/session trust boundary
**And** accepted connection metadata is associated with the registered agent identity.

**Given** socket file mode, ownership, or peer credentials do not satisfy the trust model
**When** an agent attempts to connect or register
**Then** the daemon rejects the connection or registration with a stable permission error
**And** no agent identity or capability state is created from that attempt.

**Given** an unsupported or unsafe socket form is used
**When** the daemon detects the unsafe form during startup or connection handling
**Then** it refuses the unsafe path with remediation text
**And** the refusal is visible through CLI/doctor diagnostics.

**Given** a valid connection later loses its trusted transport state or disconnects unexpectedly
**When** the daemon observes the disconnect
**Then** agent presence is updated deterministically
**And** future routing does not treat the disconnected agent as connected.

**Given** local trust conformance tests run
**When** valid user, wrong user, unsafe permission, unsupported socket form, and disconnect scenarios execute
**Then** accepted/rejected outcomes are deterministic and no unauthorized agent receives mesh traffic.

### Story 3.5: Redact Secrets Across Identity, Capability, and Delivery Surfaces

As an adopter,
I want fields marked secret to stay redacted across all observable surfaces,
So that agent coordination does not leak credentials or sensitive payload fragments.

**FR traceability:** Implemented: FR36.

**Additional traceability:** NFR-S4, NFR-S12, NFR-CA4; pins stable redaction marker and secret annotation propagation.

**Depends on:** Stories 2.1, 3.1, 3.2.

**Acceptance Criteria:**

**Given** an SDK caller marks a field or value as secret using the supported language mechanism
**When** the field flows through identity metadata, capability payloads, delivery results, errors, logs, traces, audit records, dead letters, inspect output, or CLI output
**Then** the raw secret value never appears
**And** a stable redaction marker is used where display is necessary.

**Given** a secret appears inside nested payload data or structured safe details
**When** serialization, validation, logging, or error mapping occurs
**Then** redaction is applied before the value reaches any external or persisted diagnostic surface
**And** redaction does not break schema validation of the non-secret surrounding structure.

**Given** a capability schema declares secret fields
**When** the daemon validates and stores capability metadata
**Then** the schema's secret annotations are preserved for downstream redaction decisions
**And** safe summaries omit or redact those fields.

**Given** an unsupported or ambiguous secret marker is used
**When** the SDK or daemon encounters it
**Then** the system returns a stable validation error or treats the value as secret according to the documented safe default
**And** the behavior is fixture-covered.

**Given** the shared redaction contract is introduced
**When** identity, capability, delivery, trace, audit, inspect, error, and CLI fixtures are created
**Then** supported secret markers, stable replacement text, nested redaction behavior, and ambiguous-marker defaults are pinned centrally
**And** every surface test consumes the same redaction fixture values and fails on raw secret emission.

**Given** redaction conformance tests run
**When** Rust SDK, TypeScript SDK, daemon logs, audit records, CLI inspect, metrics/traces, and dead-letter scenarios execute
**Then** no raw fixture secret appears in captured outputs
**And** tests fail on any unredacted emission.

### Story 3.6: Canonicalize Agent Identity Across Multiple Host Connections

As an adopter,
I want repeated connections from the same logical agent or host to resolve to one canonical mesh identity,
So that routing and trace history do not fragment across duplicate connection records.

**FR traceability:** Implemented: FR34.

**Depends on:** Stories 3.1, 3.4.

**Acceptance Criteria:**

**Given** the same logical agent connects through multiple supported connection paths or sessions
**When** the identity metadata matches the canonicalization rules
**Then** the daemon resolves those connections to one canonical agent identity
**And** presence output shows connection/source details without creating duplicate logical agents.

**Given** two connections claim the same identity but have incompatible metadata
**When** canonicalization evaluates them
**Then** the daemon rejects or quarantines the conflicting connection with a stable conflict error
**And** the existing canonical identity is not overwritten.

**Given** raw host identity metadata differs from the canonical mesh identity shape
**When** normalization succeeds
**Then** both raw and normalized identity forms remain available for audit/debugging
**And** routing, capability lookup, and trace output use the canonical identity.

**Given** a canonical agent has multiple active connections
**When** one connection disconnects
**Then** the agent remains present if another valid connection is active
**And** routing uses the remaining valid connection according to deterministic selection rules.

**Given** canonical identity tests run
**When** same-agent reconnect, multiple host connections, incompatible duplicate, raw/normalized storage, and partial disconnect scenarios execute
**Then** agent presence and routing behavior remain deterministic.

### Story 3.7: Connect MCP Hosts Through `zornmesh stdio --as-agent`

As a developer using an existing MCP host,
I want to connect that host to zorn-mesh through `zornmesh stdio --as-agent`,
So that existing tools can join the local mesh without host modification.

**FR traceability:** Implemented: FR33.

**Additional traceability:** NFR-C4; AR15, AR16; pins MCP 2025-11-25 compatibility fixture profile.

**Depends on:** Stories 3.1, 3.2, 3.3, 3.4, 3.5.

**Acceptance Criteria:**

**Given** an MCP-compatible host launches `zornmesh stdio --as-agent <id>`
**When** the host performs MCP initialize using the supported protocol version
**Then** the bridge completes initialization and registers the host as a mesh agent
**And** the registered identity uses the AgentCard, capability-resolution, allowlist, socket-permission, and secret-redaction contracts established by Stories 3.1-3.5.

**Given** the MCP host sends requests before successful initialize, repeats initialize, or sends messages out of the supported bridge sequence
**When** the stdio bridge validates MCP sequencing
**Then** it returns stable protocol/sequence errors without registering mesh identity or capabilities prematurely
**And** no mesh operation is dispatched until initialize and identity registration both complete.

**Given** the MCP host initializes with an unsupported protocol version
**When** the bridge validates the initialize request
**Then** it returns a stable unsupported-protocol-version error using safe MCP-compatible error details
**And** no agent identity, capability, or presence state is created.

**Given** the bridge receives MCP requests or tool calls supported by the mesh bridge
**When** it maps them into internal mesh operations
**Then** request identity, correlation ID, trace context, and capability metadata are preserved where representable
**And** unsupported mappings are not silently dropped.

**Given** the daemon is unavailable when the stdio bridge starts
**When** bridge initialization attempts mesh connection
**Then** the bridge follows the same daemon connect/auto-spawn policy as other CLI/SDK surfaces
**And** failures are reported to the host with stable, safe error information.

**Given** the host process exits or stdio closes
**When** the bridge detects closure
**Then** the corresponding mesh connection and presence state are cleaned up deterministically
**And** no orphaned agent remains visible as connected.

**Given** MCP bridge conformance tests run
**When** initialize success, out-of-sequence MCP input, duplicate initialize, unsupported protocol version, daemon unavailable, supported request mapping, policy-denied capability, redacted secret field, host exit, and malformed MCP input scenarios execute
**Then** bridge behavior is deterministic and pinned to the supported MCP version fixture set.

### Story 3.8: Degrade Gracefully for Baseline MCP Capability Limits

As a developer bridging an MCP host,
I want unsupported mesh capabilities to return explicit unsupported-capability results,
So that baseline MCP hosts fail clearly instead of appearing broken or silently losing behavior.

**FR traceability:** Implemented: FR35.

**Additional traceability:** NFR-C4; AR15; verifies unsupported-capability behavior for baseline MCP hosts.

**Depends on:** Stories 3.2, 3.3, 3.5, 3.7.

**Acceptance Criteria:**

**Given** a connected MCP host supports only baseline MCP capability shapes
**When** the bridge exposes mesh capabilities to that host
**Then** only capabilities representable on the MCP wire are exposed
**And** non-representable capabilities are withheld or marked unsupported according to documented rules.

**Given** the host invokes a mesh capability that cannot be represented on baseline MCP
**When** the bridge evaluates the invocation
**Then** it returns a named unsupported-capability result
**And** the result includes safe remediation text or equivalent CLI/SDK handoff where available.

**Given** a mesh operation partially maps to MCP but loses required semantics such as streaming, delivery ACK, trace context, or high-privilege policy
**When** the bridge evaluates the mapping
**Then** it refuses or degrades explicitly according to fixture-backed rules
**And** it does not pretend full mesh semantics were preserved.

**Given** unsupported-capability results occur
**When** the daemon and CLI surfaces observe them
**Then** they are visible as structured events and stable errors
**And** secret payload data remains redacted.

**Given** MCP graceful-degradation tests run
**When** supported capability, unsupported capability, partial mapping, policy-denied, and trace-context-limited scenarios execute
**Then** the bridge produces deterministic results pinned to the MCP version fixture set.

## Epic 4: Forensic Persistence, Trace, and Recovery

Developers can reconstruct, inspect, tail, replay, and recover multi-agent conversations from durable local evidence when something breaks.

### Story 4.1: Persist Envelopes, Audit Entries, and Trace Indexes

As a developer,
I want every accepted envelope to become durable local evidence,
So that I can later inspect, trace, replay, and audit what agents actually did.

**FR traceability:** Implemented: FR22, FR26.

**Additional traceability:** NFR-P5, NFR-R2, NFR-R3, NFR-R5, NFR-R6, NFR-CA1; AR18, AR19, AR20; pins audit record shape and SQLite recovery gates.

**Depends on:** Story 2.1 durable outcome taxonomy, Story 3.1 agent identity, and Story 3.5 redaction contract.

**Acceptance Criteria:**

**Given** the daemon accepts an envelope for durable processing
**When** the persistence writer commits it
**Then** the envelope record includes daemon sequence, message ID, source agent, target or subject, timestamp, correlation ID, trace ID, parent/lineage metadata, delivery state, and safe payload metadata
**And** durable ACK is emitted only after the relevant SQLite/sqlx transaction commits; temporary memory, queue buffering, WAL intent, or process-local cache state never counts as durable success.

**Given** the accepted envelope changes delivery or authorization state
**When** the state transition occurs
**Then** an audit entry is written with actor/agent identity, action, capability or subject, correlation ID, trace ID, prior-message lineage where available, and safe outcome details
**And** secret fields are redacted before persistence.

**Given** an envelope, delivery-state change, or authorization decision is persisted
**When** its audit entry is written
**Then** the audit row links to the relevant envelope/message ID, daemon sequence, previous audit hash, current audit hash, actor, action, state transition, and safe outcome details
**And** the envelope record, trace indexes, audit entry, and daemon sequence assignment are committed atomically or not visible as durable.

**Given** trace and correlation lookup are required by downstream trace/UI stories
**When** messages and audit entries are persisted
**Then** queryable indexes exist for correlation ID, trace ID, agent ID, subject, delivery state, and time window
**And** index naming follows the architecture conventions.

**Given** persistence is unavailable, migration state is invalid, or disk-full behavior is encountered
**When** the daemon tries to persist accepted work
**Then** the operation fails with stable typed persistence errors or enters the documented read-degraded posture
**And** no durable ACK is emitted for uncommitted work.

**Given** the daemon opens a corrupt, partially migrated, future-schema, or unreadable store
**When** startup or recovery validation runs
**Then** the daemon refuses unsafe writes or enters the documented read-degraded posture with stable typed diagnostics
**And** no durable ACK is emitted until store integrity and migration state are safe.

**Given** two daemon starts or migration workers race while schema migration is required
**When** migration locking runs
**Then** exactly one migrator applies forward-only migrations atomically
**And** failures leave pre-migration state intact while losing processes refuse startup with stable diagnostics.

**Given** the daemon crashes before, during, or after a persistence transaction
**When** it restarts
**Then** fully committed records, daemon sequences, audit hashes, and trace indexes are recovered exactly once
**And** partially committed or ambiguous work is not reported as durable and is surfaced through stable recovery diagnostics.

**Given** SQLite WAL recovery benchmarks run against the reference 7-day default-retention audit database
**When** the daemon performs startup recovery
**Then** recovery completes in <= 2 seconds on the v0.1 reference platform
**And** benchmark failures block release readiness.

**Given** persistence conformance tests run
**When** accepted envelope, commit failure, audit-hash-linkage, atomic-sequence-assignment, corrupt-store-open, redaction, indexed query, daemon restart, crash-before-after-commit, and daemon crash scenarios execute
**Then** accepted records are recoverable after restart and failed records are not reported as durable.

### Story 4.2: Propagate Tracecontext and Emit OpenTelemetry Schema

As a developer,
I want every mesh operation to carry trace context and emit documented telemetry,
So that I can follow causality across agents without instrumenting each hop by hand.

**FR traceability:** Implemented: FR29, FR30.

**Additional traceability:** NFR-O1, NFR-O2, NFR-O3; AR27.

**Depends on:** Story 4.1.

**Acceptance Criteria:**

**Given** an envelope enters the mesh with W3C `traceparent` and `tracestate` values
**When** the daemon routes, persists, delivers, retries, or dead-letters the envelope
**Then** trace context is propagated to downstream operations without adopter intervention
**And** missing trace context is generated according to documented rules.

**Given** an envelope enters with malformed `traceparent` or `tracestate`
**When** trace context is validated
**Then** malformed context is rejected or normalized according to one documented rule before routing
**And** malformed values are never propagated downstream or emitted as valid telemetry.

**Given** request/reply, streaming, publish/subscribe, fetch/lease, ACK/NACK, and cancellation operations occur
**When** telemetry is enabled for local observation or test capture
**Then** spans and metrics follow the documented `zornmesh.*` schema
**And** high-cardinality values such as correlation IDs and subjects are not emitted as metric labels.

**Given** trace data is recorded for a mesh operation
**When** the operation crosses agents or delivery states
**Then** parent/child span relationships preserve causality across the full path
**And** late, retry, replay, dead-letter, and cancellation states are represented as explicit events or attributes.

**Given** telemetry export is not configured
**When** normal daemon operations run
**Then** no outbound telemetry network connection is made
**And** local trace/audit evidence remains available for CLI and future UI surfaces.

**Given** an OpenTelemetry exporter is configured but unreachable, slow, or returning errors
**When** mesh operations emit telemetry
**Then** broker delivery, persistence, and ACK paths are not blocked beyond the documented budget
**And** exporter failures are bounded, observable through health/diagnostic events, and do not drop local audit or trace evidence.

**Given** metrics include labels derived from agents, subjects, capability IDs, error categories, or delivery states
**When** label values exceed the documented cardinality cap
**Then** excess values are bucketed or suppressed according to the telemetry schema
**And** correlation IDs, trace IDs, message IDs, raw subjects, and payload fragments never become metric labels.

**Given** observability conformance tests run
**When** trace propagation, schema validation, malformed traceparent, malformed tracestate, no-export-default, exporter unreachable, exporter slow, high-cardinality, cardinality cap, and multi-hop causality scenarios execute
**Then** output matches the documented telemetry schema and fixture expectations.

### Story 4.3: Capture Dead Letters with Structured Failure Reasons

As a developer,
I want undeliverable or exhausted envelopes to land in a dead-letter queue with clear causes,
So that failures remain inspectable and recoverable instead of disappearing.

**FR traceability:** Implemented: FR23.

**Additional traceability:** FR23; pins DLQ failure-reason taxonomy and terminal delivery state.

**Depends on:** Stories 4.1, 4.2.

**Acceptance Criteria:**

**Given** an envelope cannot be delivered because no eligible recipient exists, a TTL expires, retry budget is exhausted, validation fails after acceptance, or delivery repeatedly fails
**When** terminal failure is reached
**Then** the broker writes a dead-letter record with message ID, source, intended target/subject, correlation ID, trace ID, terminal state, failure category, and safe details
**And** the dead-letter record is persisted through the SQLite/sqlx durable store before the original envelope is considered terminal.

**Given** a dead-letter record includes payload metadata
**When** it is persisted or displayed
**Then** secret fields are redacted according to the shared redaction contract
**And** the record preserves enough metadata for trace, inspect, and future UI recovery flows.

**Given** multiple delivery attempts occurred before dead-lettering
**When** the DLQ record is created
**Then** attempt count, last failure category, and relevant timing metadata are captured
**And** retry history can be correlated to audit/trace entries.

**Given** a developer queries dead letters by subject, agent, correlation ID, failure category, or time window
**When** matching records exist
**Then** the CLI/API returns structured results with stable schema and clear empty-state behavior.

**Given** DLQ conformance tests run
**When** no-recipient, TTL-expired, retry-exhausted, validation-terminal, redaction, corrupt-store, restart-recovery, and filtered-query scenarios execute
**Then** each terminal failure creates exactly one inspectable dead-letter record.

### Story 4.4: Inspect Persistence State with Structured Filters

As a developer,
I want to inspect persisted messages, dead letters, audit entries, and runtime metadata with filters,
So that I can answer "what happened?" without opening SQLite by hand.

**FR traceability:** Implemented: FR26.

**Depends on:** Stories 4.1, 4.3.

**Acceptance Criteria:**

**Given** persisted messages, dead letters, audit entries, schema metadata, and release-integrity metadata exist or are unavailable
**When** the developer runs the inspect command or SDK/API equivalent
**Then** the response clearly distinguishes available data, unavailable data, empty states, and unsupported placeholders
**And** output is available in human and JSON modes.

**Given** the developer filters by correlation ID, trace ID, agent ID, subject, delivery state, failure category, or time window
**When** matching records exist
**Then** only matching records are returned in deterministic order
**And** filter chips/metadata in structured output explain which filters were applied.

**Given** no matching records exist
**When** an inspect query returns empty
**Then** the output explains the empty state and suggests relevant next actions such as trace, tail, doctor, or retention checks
**And** JSON mode returns an explicit empty data collection, not omitted fields.

**Given** persisted records contain redacted payloads or secret markers
**When** inspect output is rendered
**Then** raw secret values are never emitted
**And** redaction markers remain understandable in both human and JSON modes.

**Given** an inspect query could return more records than the documented default or maximum page size
**When** the CLI/API renders results
**Then** output is paginated with deterministic ordering, explicit limit metadata, and a stable next-cursor or completion marker
**And** over-limit requests return a stable validation error or are clamped according to documented rules.

**Given** inspect conformance tests run
**When** filtered message, DLQ, audit, empty, redacted, huge result set, pagination cursor, over-limit request, and unavailable-metadata scenarios execute
**Then** output shapes, ordering, stdout/stderr separation, and exit codes match fixtures.

### Story 4.5: Reconstruct Conversation Timeline by Correlation ID

As a developer debugging a broken multi-agent workflow,
I want `zornmesh trace <correlation_id>` to rebuild the ordered conversation timeline,
So that I can understand every participating agent, message, and delivery state without stitching logs by hand.

**FR traceability:** Implemented: FR24.

**Depends on:** Stories 4.1, 4.2, 4.3.

**Acceptance Criteria:**

**Given** persisted messages and audit entries share a correlation ID
**When** the developer runs `zornmesh trace <correlation_id>`
**Then** the command returns an ordered timeline containing every available envelope, hop, participating agent, delivery state, timestamp, and safe payload summary
**And** ordering is based on daemon sequence/persisted chronology, not browser or client receipt time.

**Given** the trace includes retries, late arrivals, replays, dead letters, or cancellations
**When** the timeline is rendered
**Then** each exceptional state is explicitly marked in human and JSON output
**And** the timeline does not collapse partial failure into success.

**Given** no records exist for the requested correlation ID
**When** the trace command runs
**Then** it returns a stable not-found result with remediation hints
**And** JSON output preserves the stable top-level schema with empty data and warnings.

**Given** records are missing because of retention, corruption, or partial message loss
**When** the trace command detects a gap
**Then** the output marks the trace as partial/gap detected
**And** points to inspect, doctor, retention, or audit verification next steps.

**Given** trace reconstruction tests run
**When** complete, missing, partial, retry, replay, dead-letter, and cancellation timelines execute
**Then** timeline output is deterministic and fixture-covered for both human and JSON modes.

### Story 4.6: Reconstruct Span Trees for Request/Reply and Streaming

As a developer debugging causality,
I want trace output to show parent/child span relationships for request/reply and streaming exchanges,
So that I can see which agent action caused each downstream message or stream chunk.

**FR traceability:** Implemented: FR32.

**Depends on:** Stories 4.2, 4.5.

**Acceptance Criteria:**

**Given** a request/reply exchange persists trace IDs, span IDs, parent IDs, correlation IDs, and agent references
**When** the developer requests span-tree reconstruction
**Then** the output shows parent/child relationships from initial request through reply
**And** missing or invalid parent references are explicitly marked.

**Given** a streaming exchange emits multiple chunks
**When** the span tree is reconstructed
**Then** stream chunks are grouped under the correct stream/request context in sequence order
**And** final, cancelled, failed, or gap states are represented explicitly.

**Given** a trace includes fan-out, retry, replay, or dead-letter branches
**When** the span tree is rendered
**Then** branches are labeled by relationship type such as caused-by, responds-to, replayed-from, retry-of, or dead-letter-terminal
**And** relationship labels are stable for future UI accessibility semantics.

**Given** persisted span relationships contain a self-parent, duplicate edge, or cycle
**When** span-tree reconstruction runs
**Then** the cycle is detected, traversal terminates deterministically, and the affected nodes are marked invalid/partial
**And** output does not recurse forever, drop unrelated valid branches, or invent corrected causality.

**Given** partial trace data is available
**When** parent/child reconstruction cannot be completed
**Then** the output reports partial reconstruction with safe diagnostics
**And** it does not invent or infer missing causality edges as facts.

**Given** span-tree tests run
**When** request/reply, streaming, fan-out, retry, replay, self-parent, cycle, duplicate edge, missing-parent, and partial-data scenarios execute
**Then** reconstructed causality is deterministic and fixture-covered.

### Story 4.7: Live-Tail Envelopes by Subject Pattern

As a developer,
I want to live-tail envelope flow by subject pattern,
So that I can watch the mesh in real time while agents coordinate.

**FR traceability:** Implemented: FR31.

**Depends on:** Stories 4.1, 4.2.

**Acceptance Criteria:**

**Given** a daemon is receiving envelopes
**When** the developer runs `zornmesh tail <subject-pattern>`
**Then** matching envelopes are streamed in daemon sequence order
**And** non-matching envelopes are not emitted.

**Given** the tail command runs in human mode
**When** matching events arrive
**Then** stdout shows readable event summaries with timestamp, subject, source, target or subscriber, delivery state, and correlation ID
**And** no secret payload values are displayed.

**Given** the tail command runs with JSON output
**When** matching events arrive
**Then** stdout emits NDJSON with one stable structured event per line
**And** human prose, progress text, and ANSI escape codes are not mixed into the stream.

**Given** the daemon disconnects, restarts, or falls behind during tailing
**When** the tail command detects the condition
**Then** it emits a stable disconnected/stale/backfill status according to output mode
**And** exits or reconnects according to documented behavior.

**Given** live-tail tests run
**When** matching, non-matching, JSON/NDJSON, redacted payload, daemon disconnect, and backfill scenarios execute
**Then** output ordering and mode separation match fixtures.

### Story 4.8: Redeliver Previously Sent Envelopes Safely

As a developer recovering from a failed workflow,
I want to redeliver a previously sent envelope from the audit log,
So that I can recover work without manually reconstructing payloads or hiding that replay occurred.

**FR traceability:** Implemented: FR25.

**Depends on:** Stories 4.1, 4.3, 4.5, 4.6.

**Acceptance Criteria:**

**Given** an envelope exists in the audit log and is eligible for redelivery
**When** the developer requests replay/redelivery for that envelope
**Then** the daemon creates a new delivery attempt linked to the original message
**And** the replay is clearly marked as replayed-from the original rather than treated as the original send.

**Given** the selected envelope is ineligible for redelivery because of retention, authorization, payload size, redaction, or policy limits
**When** redelivery is requested
**Then** the command returns a stable refusal reason
**And** no new delivery attempt is created.

**Given** redelivery is allowed
**When** the replayed envelope is routed
**Then** it receives a new message/delivery identity while preserving correlation and replay lineage metadata
**And** trace output can show original and replayed attempts together.

**Given** a developer requests dry-run or preview behavior before redelivery
**When** the preview is generated
**Then** the output shows target, subject, safe payload summary, replay lineage, policy checks, expected effect, and required confirmation input
**And** no delivery side effect occurs.

**Given** replay/redelivery would create a side effect
**When** the command runs in interactive, non-interactive, JSON/API, or scripted mode
**Then** redelivery requires explicit confirmation, `--yes`, or a preview-issued confirmation token according to documented mode rules
**And** missing, stale, or mismatched confirmation refuses replay without creating a delivery attempt.

**Given** redelivery tests run
**When** eligible replay, ineligible replay, preview, confirmation required, `--yes`, stale confirmation token, non-interactive refusal, replay lineage, and redaction scenarios execute
**Then** replay behavior is deterministic, auditable, and fixture-covered.

### Story 4.9: Configure Retention and Surface Retention Gaps

As an operator,
I want configurable retention for messages, dead letters, and audit records,
So that local storage remains bounded while trace gaps are explicit and explainable.

**FR traceability:** Implemented: FR27.

**Additional traceability:** NFR-SC5, NFR-CA2; AR19; pins retention-gap schema and purge audit record shape.

**Depends on:** Stories 4.1, 4.4, 4.5.

**Acceptance Criteria:**

**Given** default retention settings are active
**When** messages, dead letters, and audit records age past their configured thresholds
**Then** retention jobs purge eligible records within the documented window
**And** purge actions are themselves observable as audit/retention events.

**Given** retention purges audit entries from the middle of an audit hash chain
**When** purge work commits
**Then** purged rows are replaced by retention checkpoint/tombstone evidence containing sequence range, hash anchors, purge reason, and safe metadata
**And** offline verification can distinguish valid retention continuity from tampering without requiring raw purged payloads.

**Given** an operator configures retention by age, count, or capability class
**When** the daemon starts or reloads supported config
**Then** valid settings are applied deterministically
**And** invalid settings are rejected with stable validation errors and no partial unsafe config.

**Given** trace or inspect output references records removed by retention
**When** a developer queries the affected correlation ID or time window
**Then** the output marks a retention gap explicitly
**And** provides next-step guidance instead of returning misleading empty success.

**Given** retention sweeps run while publishes, subscriptions, trace, or inspect operations are active
**When** purge work executes
**Then** active read/write operations are not blocked beyond the documented budget
**And** no unexpired record is removed.

**Given** retention tests run
**When** default purge, configured purge, invalid config, retention gap, middle-chain purge, retention checkpoint, verify-after-retention, active read/write, and audit-of-purge scenarios execute
**Then** purging behavior is deterministic and fixture-covered.

### Story 4.10: Verify Audit Log Tamper Evidence Offline

As a compliance-minded developer or operator,
I want to verify the audit log hash chain without a running daemon,
So that I can prove local evidence has not been silently modified.

**FR traceability:** Implemented: FR28.

**Depends on:** Stories 4.1, 4.9.

**Acceptance Criteria:**

**Given** audit entries have been written by the daemon
**When** the operator runs offline audit verification against the local audit store
**Then** the verifier walks the audit hash chain and reports valid, tampered, incomplete, or unreadable status
**And** the command does not require daemon access.

**Given** a single audit row is modified, removed, reordered, or replaced
**When** offline verification runs
**Then** verification detects the tamper condition and reports the first detected break with safe diagnostics
**And** the command exits with a stable verification-failed exit code.

**Given** audit entries include redacted or personal-data-handling markers
**When** verification runs
**Then** redaction markers preserve chain verifiability
**And** raw secret values are not required or emitted by the verifier.

**Given** audit retention checkpoints or tombstones exist
**When** offline verification walks the audit store
**Then** the verifier preserves hash-chain continuity across retained segments
**And** reports valid retention gaps separately from tamper, corruption, or missing data.

**Given** the audit store is missing, locked, unreadable, or from an unsupported future schema
**When** verification runs
**Then** the command returns a stable structured error
**And** remediation text distinguishes missing data from tamper evidence.

**Given** audit verification tests run
**When** valid chain, modified row, deleted row, reordered row, redacted chain, retention checkpoint, valid retention gap, missing store, and unsupported schema scenarios execute
**Then** offline verification behavior is deterministic and fixture-covered.

## Epic 5: Compliance, Audit, and Release Trust Evidence

Operators and compliance reviewers can verify release integrity, export evidence, prove audit-log integrity, handle redaction/deletion, and map events to required AI-risk/compliance frameworks.

### Story 5.1: Produce and Verify Release Signatures, SBOMs, and Reproducibility Evidence

As an operator,
I want to verify the installed `zornmesh` artifact signature and retrieve its SBOM,
So that I can trust what binary or SDK package is running in my local environment.

**FR traceability:** Implemented: FR37, FR38; Verified: FR37, FR38.

**Additional traceability:** NFR-S5, NFR-M1, NFR-M6, NFR-CA5; AR28; pins release signatures, SBOM, provenance, and reproducibility evidence.

**Acceptance Criteria:**

**Given** the v0.1 release pipeline builds Linux and macOS artifacts and SDK packages
**When** release preflight runs
**Then** every artifact has a Sigstore signature, CycloneDX SBOM, dependency inventory, provenance metadata, and reproducibility report where the toolchain permits
**And** missing signature, missing SBOM, unaccounted dependency, or non-reproducible reference build status fails release readiness instead of being deferred to install-time verification.

**Given** a signed `zornmesh` release artifact is installed
**When** the operator runs the release verification command or doctor check
**Then** the command verifies the artifact against the published Sigstore signature
**And** reports verified, unverifiable, missing-signature, or mismatch states with stable exit codes.

**Given** the installed artifact has an associated CycloneDX SBOM
**When** the operator runs `zornmesh inspect sbom` or equivalent structured command
**Then** the SBOM is returned in the documented format
**And** JSON output can be consumed without human prose mixed into stdout.

**Given** a source-built installation is used
**When** SBOM generation or lookup runs
**Then** the command reports whether the SBOM was generated at install/build time or is unavailable
**And** unavailable SBOM status is explicit rather than treated as success.

**Given** signature or SBOM verification fails
**When** the operator inspects diagnostics
**Then** the output includes safe remediation guidance
**And** no network fetch or remote trust decision occurs unless explicitly configured by the operator.

**Given** release-integrity tests run
**When** valid signature, missing signature, mismatched artifact, valid SBOM, missing SBOM, and JSON output scenarios execute
**Then** verification behavior is deterministic and fixture-covered.

### Story 5.2: Enforce Compliance Traceability Fields on Envelopes

As a compliance reviewer,
I want every evidence-bearing envelope to carry required traceability fields,
So that agent actions can be mapped to who acted, what capability was used, and what prior message caused it.

**FR traceability:** Implemented: FR41.

**Acceptance Criteria:**

**Given** an agent sends, receives, replies, streams, ACKs, NACKs, replays, or triggers a dead-letter state
**When** the daemon records the envelope or related audit event
**Then** the record includes agent identity, capability or subject, timestamp, correlation ID, trace ID, and prior-message lineage where applicable
**And** missing required traceability fields produce stable validation or evidence-gap results.

**Given** an envelope uses a capability descriptor
**When** the evidence record is written
**Then** the capability identifier and version are preserved in safe evidence metadata
**And** high-privilege capability decisions link to their authorization outcome.

**Given** a traceability field contains sensitive data or references redacted payload material
**When** evidence is rendered or exported
**Then** raw sensitive values are redacted while stable identifiers and lineage remain verifiable.

**Given** legacy, partial, or bridge-originated records cannot provide all fields
**When** compliance traceability validation runs
**Then** the record is marked with an explicit evidence-gap reason
**And** the system does not silently claim compliance completeness.

**Given** compliance traceability tests run
**When** normal send, high-privilege invoke, replay, DLQ, MCP-bridge, missing-field, and redacted-field scenarios execute
**Then** traceability fields and evidence-gap behavior are deterministic and fixture-covered.

### Story 5.3: Export Evidence Bundle for a Time Window

As a compliance reviewer,
I want to export a self-contained evidence bundle for a time window,
So that I can review local agent activity, release integrity, and configuration posture without manual file gathering.

**FR traceability:** Implemented: FR42.

**Additional traceability:** NFR-CA3; depends on Stories 4.1, 4.9, 5.1, and 5.2; owns audit export duration gate and bundle manifest schema.

**Acceptance Criteria:**

**Given** audit, trace, signature, SBOM, and configuration evidence exists for a requested time window
**When** the reviewer runs the evidence export command with `--since` and `--until`
**Then** the command emits a self-contained bundle containing audit-log slice, trace/correlation references, SBOM, signature verification status, and sanitized config snapshot
**And** the bundle includes a manifest describing included sections and evidence gaps.

**Given** a 7-day evidence window is exported on the v0.1 reference machine
**When** audit export runs
**Then** the export completes within 5 minutes
**And** the manifest records duration and any performance-limit evidence gaps.

**Given** the requested time window includes retained and purged data
**When** export runs
**Then** retained data is included and purged portions are marked as retention gaps
**And** the export does not represent missing records as complete evidence.

**Given** evidence contains secrets or personal-data redaction markers
**When** the bundle is generated
**Then** raw secrets are not emitted
**And** redaction/proof markers remain sufficient for audit-chain and traceability review.

**Given** export cannot complete because of unreadable store, invalid time window, missing release metadata, or unsupported schema
**When** the command fails
**Then** it returns a stable structured error
**And** no partial bundle is reported as complete.

**Given** evidence export tests run
**When** complete export, incident-review export, release-review export, retention gap, redacted export, missing SBOM/signature, invalid time window, and store error scenarios execute
**Then** exported bundle content and manifest are deterministic and fixture-covered.

### Story 5.4: Redact Personal Data While Preserving Audit Integrity

As a subject data owner or compliance reviewer,
I want personal data referenced in envelopes to be redacted through a documented procedure,
So that privacy obligations can be met without destroying audit-chain integrity.

**FR traceability:** Implemented: FR43.

**Acceptance Criteria:**

**Given** a documented redaction request identifies a subject, time window, correlation ID, or record scope
**When** the operator runs the redaction command
**Then** matching personal-data fields are replaced with redaction markers
**And** non-personal traceability fields such as correlation IDs, trace IDs, timestamps, and lineage remain available where policy permits.

**Given** redaction affects audit-relevant records
**When** the redaction is applied
**Then** existing audit-chain entries and prior hashes are never rewritten, deleted, or re-linked
**And** redaction is represented by append-only tombstone/redaction-marker records, a durable scope checkpoint, and a `REDACTION_APPLIED` proof record referencing original record IDs/hashes, actor, timestamp, policy/version, and redaction scope.
**And** offline audit verification validates chain continuity through the checkpoint/proof records and distinguishes authorized redaction from missing, deleted, reordered, or tampered rows.

**Given** matching records are being written while a redaction request runs
**When** redaction begins
**Then** the operation establishes a durable cutoff/checkpoint for the redaction scope
**And** records at or before the checkpoint are redacted or explicitly refused atomically, while post-checkpoint matching records are blocked, queued for follow-up, or reported as requiring a subsequent redaction run.
**And** no in-flight matching record can silently bypass redaction.

**Given** the requested redaction scope is invalid, too broad, outside retention, or conflicts with immutable evidence policy
**When** redaction is requested
**Then** the command returns a stable refusal or evidence-gap result
**And** no partial redaction is reported as complete.

**Given** redacted records are later inspected, traced, exported, or dead-lettered
**When** those surfaces render the records
**Then** redaction markers appear consistently
**And** raw personal data does not reappear from cached, indexed, or derived fields.

**Given** redaction tests run
**When** valid redaction, invalid scope, retention gap, authorized redaction proof, unauthorized tamper attempt, concurrent matching write, checkpoint cutoff, post-checkpoint follow-up required, trace after redaction, export after redaction, and cache/index scenarios execute
**Then** redaction behavior is deterministic and fixture-covered.

### Story 5.5: Map Envelopes to NIST AI RMF Functions and Categories

As a risk reviewer,
I want local mesh events mapped to NIST AI RMF functions and categories,
So that audits can connect concrete runtime evidence to recognized AI risk-management controls.

**FR traceability:** Implemented: FR44.

**Acceptance Criteria:**

**Given** envelopes, audit events, capability decisions, redaction events, and release evidence are persisted
**When** the reviewer runs the AI RMF mapping report
**Then** each included evidence type is mapped to the applicable Govern, Map, Measure, and Manage function/category references
**And** unmapped evidence is explicitly marked rather than silently omitted.

**Given** an evidence record lacks required metadata for a confident AI RMF mapping
**When** the report is generated
**Then** the record is included with an evidence-gap reason
**And** the report does not claim full control coverage for that record.

**Given** AI RMF mapping definitions are versioned
**When** the report is generated
**Then** the output records the mapping-definition version, schema version, generation time, and input evidence window
**And** prior fixtures remain reproducible across mapping-definition updates.

**Given** an authorized reviewer needs to override an unmapped or incorrect AI RMF mapping
**When** the reviewer submits a manual override
**Then** the workflow requires actor identity, evidence reference, previous mapping, requested mapping, mapping-definition version, reason, timestamp, and review/expiry status
**And** the override validates that the target function/category exists without modifying the original evidence record.

**Given** a manual AI RMF override is accepted, rejected, expired, or superseded
**When** audit evidence is persisted
**Then** an append-only audit record captures actor/session, source evidence ID, before/after mapping, reason, mapping-definition version, decision outcome, and timestamp
**And** reports distinguish automatic mappings, manual overrides, unmapped evidence, and evidence-gap records.

**Given** the report is exported as part of a compliance bundle
**When** the evidence bundle is opened offline
**Then** AI RMF mappings, evidence gaps, and source trace references are reviewable without network access
**And** redacted records preserve mapping context without exposing raw protected data.

**Given** AI RMF mapping tests run
**When** complete coverage, unmapped evidence, missing metadata, mapping-version drift, redacted records, manual override accepted, manual override rejected, override audit log, override version drift, and offline bundle scenarios execute
**Then** mapping output is deterministic and fixture-covered.

## Epic 6: Local Web Control Room and Safe Intervention

Developers can open the local UI, observe connected agents, inspect trace chronology, send direct/broadcast messages safely, confirm outcomes, reconnect/backfill, and copy CLI handoffs.

**Dependency gate:** Epic 6 is not implementation-ready, and Stories 6.2-6.9 must not begin, until Story 6.1 (a) verifies and references — by section — the existing v0.1 local UI architecture amendment that already supersedes earlier no-GUI/frontend/static-asset text, (b) pins the local UI framework wording so no Node-served runtime, hosted serving model, or remote-asset dependency can enter v0.1, and (c) scaffolds the local web app shell, shared UI/API taxonomies, component fixture baseline, and scope-boundary checks against that referenced architecture.

### Story 6.1: Verify Local UI Architecture, Pin Framework Wording, and Scaffold Local Web App Shell

As a developer,
I want Story 6.1 to verify the existing local UI architecture amendment, pin the framework wording, and scaffold the local web app shell before feature work begins,
So that implementation follows the validated PRD/UX/architecture scope and v0.1 cannot silently introduce a Node-served runtime, hosted serving model, or remote-asset dependency.

**FR traceability:** Supported: FR49, FR50, FR51, FR52, FR53, FR54, FR55, FR56, FR57, FR58, FR59, FR60; Gated: FR49, FR50, FR51, FR52, FR53, FR54, FR55, FR56, FR57, FR58, FR59, FR60.

**Additional traceability:** NFR-C6, NFR-C7, NFR-M7, NFR-A11Y1-NFR-A11Y6; UX-DR1-UX-DR9, UX-DR21, UX-DR22, UX-DR23; pins shared UI/API taxonomies.

**Acceptance Criteria:**

**Given** the architecture artifact already contains the v0.1 local UI amendment that supersedes earlier no-GUI/frontend/static-asset text
**When** Story 6.1 is completed
**Then** Story 6.1 cites the existing amendment by section reference (architecture supersession note, Local UI scope decision, and the local web companion UI section) and links Epic 6 planning to those sections
**And** Story 6.1 pins the v0.1 local UI framework wording to "Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only," and records that v0.1 ships no Node-served runtime, no hosted serving model, no Next.js server features, and no remote browser assets
**And** Story 6.1 records that hosted/cloud dashboard, LAN/public console, accounts/teams, full chat workspace, workflow editor, remote browser assets, and external runtime services remain out of scope, consistent with NFR-S8, NFR-S10, NFR-S11, and NFR-C7.

**Given** the local UI shell is scaffolded
**When** the developer runs the documented UI build/test entrypoint
**Then** a Bun-managed React app shell exists for the `zornmesh ui` surface, produces statically bundled assets that are served only by the daemon UI gateway on loopback, and introduces no Node-served runtime, no Next.js server features, no remote-asset dependency, and no external runtime service
**And** Tailwind-aligned styling, Radix-style accessible primitive composition, and project-owned UI primitive wrappers are established without adding unsupported package managers.

**Given** foundational UI tokens are defined
**When** the shell renders fixture states
**Then** dark-first graphite/charcoal, electric-blue actions, cyan local-trust accents, semantic success/warning/error/neutral states, typography, spacing, radius, borders, focus rings, and light-mode support are available through project-owned tokens
**And** technical strings such as agent IDs, trace IDs, subjects, timestamps, CLI commands, and payload metadata use readable monospace styling.

**Given** shared state language exists across CLI, SDK, daemon, and UI surfaces
**When** the UI shell renders initial fixture data
**Then** agent status, delivery state, trace completeness, daemon health, and trust posture taxonomies are represented as shared UX/API contracts
**And** unknown or future states render explicit fallback labels rather than silent success states.

**Given** UI component fixtures are seeded before full daemon integration
**When** the fixture test suite runs
**Then** buttons, inputs, dialogs, popovers, tooltips, tabs, menus, toasts, badges, panels, and layout primitives render deterministic baseline, loading, error, disabled, focus, and reduced-motion states
**And** fixture failures point to the affected component/state contract before dependent UI feature stories proceed.

**Given** UI routes, navigation, and actions are scaffolded
**When** scope-boundary checks run
**Then** the shell exposes only observe, inspect, reconnect/backfill, safe direct send, safe broadcast, outcome review, and CLI handoff surfaces
**And** workflow editing, full chat orchestration, cloud sync, LAN/public serving, account/team management, and remote dashboard behavior are absent or return explicit out-of-scope errors.

### Story 6.2: Launch Protected Loopback UI with Offline Assets

As a developer,
I want `zornmesh ui` to launch a protected local web UI,
So that I can inspect and operate the mesh from a browser without exposing the control surface beyond my machine.

**FR traceability:** Implemented: FR49, FR58.

**Additional traceability:** NFR-S9, NFR-S10, NFR-S11, NFR-C7; UX-DR18; owns protected-loopback and offline-asset gate.

**Acceptance Criteria:**

**Given** the local daemon is available and the UI feature is enabled
**When** the developer runs `zornmesh ui`
**Then** the command starts or connects to the local UI server on loopback only
**And** it either opens a browser window or prints a protected loopback URL suitable for copy/paste.

**Given** the preferred UI port is already bound by another process
**When** `zornmesh ui` starts
**Then** the command either selects a documented alternate loopback port or fails with a stable `UI_PORT_IN_USE` error and remediation
**And** it never sends session tokens to, proxies through, or treats the existing process as trusted.

**Given** the browser opens the local UI URL
**When** the session is established
**Then** access requires a per-launch high-entropy session token or one-time code with bounded lifetime, server-side revocation on shutdown/expiry, and no persistence in localStorage
**And** token-bearing material is removed from browser history after exchange, omitted from logs/audit payloads/CLI handoff text, protected with `Referrer-Policy: no-referrer`, and not leaked through referrer headers.

**Given** browser requests reach the UI API or live event transport
**When** HTTP, WebSocket, or SSE requests are made
**Then** CORS denies by default except the exact loopback origin, Origin/Host checks fail closed, and WebSocket/SSE upgrades require the same session protection as HTTP
**And** state-changing requests require CSRF protection bound to the server-side session and derive actor/session identity on the server rather than trusting browser-supplied actor fields.

**Given** UI assets are served
**When** the browser loads the app
**Then** JavaScript, CSS, fonts, icons, and fixture assets are bundled for offline use
**And** the app makes no external browser network requests for runtime assets, telemetry, fonts, or analytics.

**Given** the daemon, session, or local trust state changes
**When** the UI shell renders status chrome
**Then** it displays daemon health, loopback-only status, session protection, socket path, schema version, bundled/offline asset indicator, and stale/disconnected/session-expired warnings
**And** critical status is communicated with text/icon/shape, not color alone.

**Given** launch and security tests run
**When** open-browser, printed-URL, port in use, invalid token, missing token, token expiry, token history cleanup, referrer leak prevention, unsafe origin, CORS rejection, CSRF failure, WebSocket/SSE unauthenticated upgrade, actor/session binding, non-loopback bind attempt, offline asset, and daemon-unavailable scenarios execute
**Then** launch behavior and failures are deterministic and fixture-covered.

### Story 6.3: Render Live Agent Roster and Local Trust Status

As a developer,
I want the Live Mesh Control Room to show connected agents and daemon trust state,
So that I can quickly understand who is participating in the local mesh and whether the control room is safe to use.

**FR traceability:** Implemented: FR50, FR58.

**Additional traceability:** NFR-P7; UX-DR14, UX-DR15, UX-DR18, UX-DR26, UX-DR28.

**Acceptance Criteria:**

**Given** registered agents and capability summaries are available from the daemon
**When** the Live Mesh Control Room loads
**Then** the roster shows each agent's display name, stable ID, status, transport/source, capability summary, last-seen recency, recent activity count, and warning markers
**And** MCP stdio, native SDK, stale, errored, disconnected, and reconnecting states are visibly distinct.

**Given** a developer selects an agent
**When** the agent detail or capability card opens
**Then** it shows identity, transport, capabilities, subscriptions, recent traces, activity, trust indicators, permission indicators, and high-privilege warnings
**And** unavailable or denied high-privilege capabilities are explained without enabling unsafe actions.

**Given** the roster has many agents or mixed states
**When** the developer searches, filters, or highlights agents by ID, name, capability, status, warning, source, or recent trace
**Then** matching agents remain findable without changing message chronology
**And** active filters are visible and removable.

**Given** roster or daemon state is empty, loading, stale, degraded, unavailable, or session-expired
**When** the control room renders
**Then** persistent state panels explain the condition and next action
**And** transient toasts never replace persistent status for critical trust or availability issues.

**Given** roster fixture tests run
**When** empty roster, active agents, stale agents, disconnected agents, MCP/native source, high-privilege warning, filtered roster, unavailable daemon, and session-expired scenarios execute
**Then** roster and trust-state rendering are deterministic and fixture-covered.

**Given** the 3-agent roster fixture runs after daemon readiness
**When** the Live Mesh Control Room renders connected agents
**Then** agent roster and local trust status are visible within 2 seconds on the v0.1 reference browser profile
**And** failures emit stable UI performance evidence.

### Story 6.4: Render Daemon-Sequence Timeline and Event Detail Panel

As a developer,
I want a daemon-sequenced trace timeline with event details,
So that I can understand message flow and delivery state without stitching logs together manually.

**FR traceability:** Implemented: FR51, FR52.

**Additional traceability:** NFR-P8; UX-DR10-UX-DR13, UX-DR30.

**Acceptance Criteria:**

**Given** trace and message events are available from the daemon
**When** the control room renders the timeline
**Then** events are ordered by daemon sequence as the primary chronology
**And** browser receipt time appears only as secondary diagnostic metadata.

**Given** timeline events include causality and delivery metadata
**When** events render
**Then** each row shows event summary, sender/recipient, subject or operation, daemon sequence, timestamp, causal marker, delivery state badge, keyboard selection, and expansion/selection affordance
**And** pending, queued, accepted, delivered, acknowledged, rejected, failed, cancelled, replayed, dead-lettered, stale, and unknown states use consistent labels.

**Given** a developer selects a trace event
**When** the detail panel opens
**Then** it shows event summary, sender, recipients, subject, correlation ID, daemon sequence, timestamp, parent/child links, payload metadata, delivery outcome, timing, source/target agent, suggested next action, and copyable relevant command where available
**And** selected detail remains stable while new live events arrive.

**Given** a trace exceeds the browser rendering or memory budget
**When** the timeline loads
**Then** the UI requests daemon-sequence pages/windows and renders a virtualized timeline rather than materializing the full trace in the DOM
**And** loaded range, total/unknown count, gaps, and partial-window state are visible without changing daemon-sequence ordering.

**Given** a trace contains <= 500 events
**When** the timeline and selected-event detail render on the reference browser profile
**Then** ordered timeline and selected detail are visible within 1 second
**And** performance evidence is recorded with the fixture result.

**Given** late, reconstructed, replayed, missing-parent, gap, or dead-letter states appear
**When** the timeline and detail panel render those events
**Then** each state is labeled with accessible text and a recovery/inspection cue
**And** the UI does not imply a complete trace when evidence gaps exist.

**Given** timeline/detail tests run
**When** complete trace, partial trace, missing parent, late event, replayed event, dead letter, keyboard navigation, live append, selected-detail stability, large trace virtualization, paged timeline window, partial-window gap markers, and unknown delivery state scenarios execute
**Then** timeline ordering and detail rendering are deterministic and fixture-covered.

### Story 6.5: Open Focused Trace Reader with CLI Handoff Commands

As a developer debugging a multi-agent conversation,
I want to open a focused trace reader for one correlation ID and copy matching CLI handoff commands,
So that I can move between visual inspection and terminal investigation without losing context.

**FR traceability:** Implemented: FR53, FR59.

**Acceptance Criteria:**

**Given** a trace event has a correlation ID
**When** the developer opens the focused trace reader
**Then** the UI shows the full known conversation when it is within the browser window budget, or a clearly labeled daemon-sequence window with load-more controls when it is too large
**And** parent/child links, caused-by/responds-to/replayed-from/broadcast fan-out labels, delivery states, payload summaries, timing, and focus/pause behavior preserve daemon-sequence ordering.

**Given** trace evidence is incomplete, reconstructed, stale, dead-lettered, or has missing parents
**When** the focused trace reader renders
**Then** it labels the gap or recovery state in accessible text
**And** it provides a guided recovery cue such as inspect trace, inspect dead letter, replay, reconnect, or audit verification where applicable.

**Given** the current context supports terminal follow-up
**When** the developer opens a CLI command copy block
**Then** commands for trace, inspect, replay, agents, doctor, and audit operations include context-preserving arguments such as correlation ID, agent ID, daemon sequence, time window, or evidence path
**And** each command includes a description, expected outcome, copy action, and copied feedback.

**Given** context values contain spaces, quotes, semicolons, backticks, dollar signs, newlines, glob characters, option-like prefixes, or shell metacharacters
**When** CLI handoff commands are generated
**Then** commands are constructed from argv tokens with documented shell quoting/escaping and never by unsafe string concatenation
**And** copied commands cannot introduce command substitution, redirection, chaining, environment assignment, or extra arguments from untrusted trace, agent, subject, or evidence values.

**Given** a command is unavailable because the daemon is offline, audit evidence is missing, context is insufficient, or the operation would be unsafe
**When** the command block renders
**Then** it shows an unavailable/requires-daemon/offline-audit explanation
**And** no misleading command is offered.

**Given** focused trace tests run
**When** complete trace, missing trace, reconstructed trace, broadcast fan-out, dead-letter event, command copy, unavailable command, large focused trace, shell metacharacter escaping, option-like IDs, newline-bearing context values, pause/focus, and return-to-control-room scenarios execute
**Then** the focused reader and CLI handoff behavior are deterministic and fixture-covered.

### Story 6.6: Send Safe Direct Messages from the UI

As a developer,
I want to send a direct message to one selected agent only after reviewing target identity and payload preview,
So that human-originated UI sends are intentional, validated, and auditable.

**FR traceability:** Implemented: FR54, FR56, FR60.

**Additional traceability:** NFR-P9; UX-DR16, UX-DR23, UX-DR24, UX-DR25.

**Acceptance Criteria:**

**Given** a developer selects one target agent
**When** the safe composer opens in direct mode
**Then** it shows target display name, stable agent ID, transport/source, status, capability summary, high-privilege warnings, message body, subject/operation where applicable, payload preview, and validation state
**And** direct mode is visually and textually distinct from broadcast mode.

**Given** the target is stale, disconnected, missing required capability, denied by allowlist, unavailable because the daemon is offline, or the message body/subject is invalid
**When** the developer attempts to send
**Then** the send action is blocked with an explanatory validation error
**And** disabled actions explain why without requiring hover-only affordances.

**Given** the direct message is valid
**When** the developer sends it
**Then** the UI prevents duplicate submission while pending and displays queued, delivered, acknowledged, rejected, timed-out, or dead-lettered outcome states as they arrive
**And** persistent outcome display is not replaced by a transient toast.

**Given** a valid direct send targets one available recipient
**When** the daemon accepts the send
**Then** terminal delivery outcome is displayed within 5 seconds unless the explicit agent timeout policy exceeds that budget
**And** over-budget pending state remains visible with the active timeout policy.

**Given** the direct send is accepted by the daemon
**When** audit evidence is persisted
**Then** the record links actor/session, target recipient, trace/correlation ID, payload summary, validation outcome, and delivery outcome
**And** raw secrets or protected payload fields are not exposed in UI audit summaries.

**Given** direct-send tests run
**When** valid send, invalid recipient, stale recipient, denied capability, empty body, invalid subject, daemon unavailable, duplicate click, rejected send, timed-out send, dead-lettered send, and audit-link scenarios execute
**Then** direct composer behavior and audit linkage are deterministic and fixture-covered.

### Story 6.7: Confirm Broadcast Scope and Show Per-Recipient Outcomes

As a developer,
I want broadcasts to require explicit scope review and show per-recipient outcomes,
So that broad-impact sends are deliberate and failures are visible for each target.

**FR traceability:** Implemented: FR55, FR56, FR60.

**Additional traceability:** NFR-P9; UX-DR17, UX-DR23, UX-DR24, UX-DR31.

**Acceptance Criteria:**

**Given** the developer chooses broadcast mode
**When** recipients are selected or resolved from capability/status filters
**Then** the UI shows recipient count, included recipients, excluded or incompatible recipients, payload summary, capability warnings, and unsafe-scope warnings
**And** broadcast mode is visually and textually distinct from direct mode.

**Given** a broadcast would affect multiple recipients
**When** the developer attempts to send
**Then** a confirmation dialog names the scope, recipient count, excluded/incompatible recipients, and payload summary
**And** the final confirmation requires explicit user action and avoids modal chains.

**Given** recipient membership, capability compatibility, or agent status changes after the confirmation preview is shown
**When** the developer confirms the broadcast
**Then** the daemon validates the confirmed recipient snapshot/revision before accepting the send
**And** any recipient-set drift blocks the send, refreshes the preview, and requires explicit reconfirmation.

**Given** the confirmed broadcast is sent
**When** delivery outcomes arrive
**Then** the UI displays a persistent per-recipient outcome list with queued, delivered, acknowledged, rejected, timed-out, dead-lettered, pending, stale, partial-success, all-failed, and success states as applicable
**And** failure reasons, timing, retry affordance, and inspect affordance are available per recipient where safe.

**Given** a valid broadcast targets three recipients
**When** the daemon accepts the send
**Then** terminal per-recipient outcomes are displayed within 5 seconds unless explicit agent timeout policy exceeds that budget
**And** pending or partial states remain visible until every recipient reaches a terminal or policy-defined timeout state.

**Given** the broadcast is accepted by the daemon
**When** audit evidence is persisted
**Then** the record links actor/session, requested recipient scope, previewed recipient snapshot, accepted recipient snapshot/revision, actual recipient list, excluded recipients, drift/reconfirmation outcome, trace/correlation ID, payload summary, and per-recipient delivery outcomes
**And** partial failure remains visible in both UI and audit evidence.

**Given** broadcast tests run
**When** successful broadcast, partial failure, all failed, excluded recipients, incompatible recipients, stale recipients, unsafe scope, confirmation cancel, recipient drift after preview, reconfirmation required, stale snapshot rejected, duplicate submit, retry/inspect affordance, and audit-link scenarios execute
**Then** broadcast confirmation and per-recipient outcome behavior are deterministic and fixture-covered.

### Story 6.8: Reconnect, Backfill, and Preserve UI Context

As a developer,
I want the UI to recover after refreshes, reconnects, and daemon restarts,
So that I can continue investigating the same trace without losing chronology or mistaking gaps for complete evidence.

**FR traceability:** Implemented: FR57.

**Additional traceability:** NFR-R7; UX-DR20, UX-DR30, UX-DR36; depends on Story 4.9 retention gaps.

**Acceptance Criteria:**

**Given** the developer refreshes the browser or the UI reconnects after a transient disconnect
**When** the session is still valid
**Then** the UI restores the selected trace, selected agent where possible, active filters, and current view mode after daemon backfill completes
**And** restored events remain ordered by daemon sequence.

**Given** the daemon restarts, becomes unavailable, reconnects, or reports a schema/session change
**When** the UI detects the transition
**Then** persistent status chrome shows starting, reconnecting, degraded, unavailable, stale, schema-mismatch, or session-expired state as applicable
**And** actions that cannot safely run are disabled with explanatory text.

**Given** backfill includes retained, purged, late, duplicated, or reconstructed events
**When** the UI merges live and backfilled data
**Then** it deduplicates by stable daemon/event identity, marks retention gaps and late/reconstructed events, and preserves selected detail stability
**And** the UI never reorders trace events by browser receipt time.

**Given** reconnect/backfill would exceed browser capacity
**When** the UI restores context
**Then** it restores the selected trace as a daemon-sequence window around the prior selection and marks the view partial until additional pages load
**And** actions and status copy do not imply the entire trace is loaded.

**Given** reconnect or backfill cannot complete
**When** the developer views the affected trace or roster state
**Then** the UI shows an evidence-gap or recovery panel with safe next actions such as retry reconnect, inspect trace by CLI, inspect daemon health, or export audit evidence
**And** partial state remains visible rather than being replaced by a generic failure page.

**Given** reconnect/backfill tests run
**When** browser refresh, transient disconnect, daemon restart, unavailable daemon, schema mismatch, expired session, retained plus purged data, late event, duplicate event, selected-detail stability, and failed backfill scenarios execute
**Then** reconnect behavior and context preservation are deterministic and fixture-covered.

### Story 6.9: Prove Accessibility, Responsive Behavior, and Browser Fixture Coverage

As a developer or QA reviewer,
I want the local web UI's critical journeys verified across accessibility, responsive layouts, and supported browsers,
So that the control room remains usable and trustworthy for real debugging sessions.

**FR traceability:** Verified: FR49, FR50, FR51, FR52, FR53, FR54, FR55, FR56, FR57, FR58, FR59, FR60; Gated: FR49, FR50, FR51, FR52, FR53, FR54, FR55, FR56, FR57, FR58, FR59, FR60.

**Additional traceability:** NFR-C6, NFR-C7, NFR-M7, NFR-A11Y1-NFR-A11Y6; UX-DR32-UX-DR36; verifies but does not implement FR49-FR60.

This story is a quality/readiness gate for UI features implemented by Stories 6.2-6.8; accessibility, responsive, and browser fixture evidence must not be used to claim feature implementation.

**Acceptance Criteria:**

**Given** the local UI critical journeys exist for roster inspection, trace timeline navigation, trace detail, focused trace reader, direct send, broadcast confirmation, outcome review, reconnect/backfill, and CLI command copy
**When** accessibility checks run
**Then** automated checks, keyboard-only walkthroughs, focus-order checks, screen-reader spot checks, reduced-motion checks, color-blindness checks, and no-color-only verification pass for those journeys
**And** failures identify the affected journey and component state.

**Given** the UI runs at mobile, tablet, desktop, and wide desktop breakpoints
**When** responsive fixture tests execute
**Then** the layout uses one-pane mobile, two-pane tablet, and three-pane desktop behavior as specified
**And** no responsive mode reorders trace events, hides delivery/failure state, or loses selected detail stability.

**Given** technical data contains long agent IDs, correlation IDs, subjects, timestamps, payload summaries, CLI commands, and error messages
**When** the UI renders across supported breakpoints
**Then** text remains readable, copyable where appropriate, and does not break timeline chronology, controls, or status visibility
**And** truncation or wrapping preserves accessible labels.

**Given** browser E2E coverage runs for supported local browsers
**When** Chromium, Firefox, and WebKit/Safari-compatible scenarios execute
**Then** fixture-backed journeys for complete, partial, missing, reconstructed, and live traces; stale/disconnected agents; daemon unavailable; session expired; direct send; broadcast success; broadcast partial failure; validation-blocked send; late event arrival; and backfill are covered
**And** browser differences are represented as explicit unsupported-state or defect evidence rather than ignored.

**Given** browser fixture evidence is collected for current stable Chromium, Firefox, and Safari/WebKit
**When** a browser-specific behavior differs or fails
**Then** the fixture records explicit unsupported-state or defect evidence
**And** cross-browser gaps cannot be silently ignored in release readiness.

**Given** release/readiness checks include UI quality gates
**When** the UI test suite completes
**Then** accessibility, responsive, browser, offline-asset, and critical-journey fixture results are emitted as stable evidence for implementation readiness
**And** the suite fails explicitly when required browser or accessibility tooling is unavailable.
