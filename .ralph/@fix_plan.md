# Ralph Fix Plan

## Stories to Implement

### First Local Mesh and SDK Bootstrap
> Goal: A developer can install/run `zornmesh`, start or auto-spawn one trustworthy local daemon, use stable CLI output, and send/receive a first basic envelope through Rust/TypeScript SDK surfaces.

- [x] Story 1.1: Create Buildable Workspace and Command Skeleton
- [x] Story 1.2: Establish Local Daemon Rendezvous and Trust Checks
- [x] Story 1.3: Connect Rust SDK to Auto-Spawned Daemon
- [x] Story 1.4: Send First Local Publish/Subscribe Envelope
- [x] Story 1.5: Add TypeScript SDK Bootstrap Parity
- [x] Story 1.6: Stabilize CLI Read Outputs and Exit Contracts
- [x] Story 1.7: Provide Doctor, Shutdown, and Shell Completion Basics
### Reliable Agent Coordination
> Goal: Agents can coordinate beyond the first message using request/reply, pull leases, streaming, ACK/NACK, cancellation, idempotency, durable subscriptions, backpressure, and per-call context. **Durability contract:** Stories 2.1-2.8 may claim durable ACK, lease, idempotency, subscription, retry, or backpressure state only after the relevant SQLite/sqlx commit succeeds. In-memory-only state must return typed persistence-unavailable or unsupported outcomes and must never claim durable success.

- [x] Story 2.1: Establish Coordination Result and ACK/NACK Contract
- [x] Story 2.2: Send Correlated Request/Reply with Timeout
- [x] Story 2.3: Fetch, Lease, ACK, and NACK Pulled Envelopes
- [x] Story 2.4: Add Idempotency Keys and Retry-Safe Sends
- [x] Story 2.5: Stream Response Chunks with Byte-Budget Flow Control
- [x] Story 2.6: Cancel In-Flight Requests and Streams
- [x] Story 2.7: Resume Durable Subscriptions After Daemon Restart
- [x] Story 2.8: Surface Backpressure at Queue and Lease Bounds
### Agent Identity, Capabilities, and Host Bridges
> Goal: Developers can see who is on the mesh, what each agent can do, safely gate high-privilege capabilities, and bridge existing MCP hosts without modifying those hosts.

- [x] Story 3.1: Register Minimal AgentCard Identity
- [x] Story 3.2: Advertise and Resolve Symmetric Capabilities
- [x] Story 3.3: Gate High-Privilege Capabilities by Local Allowlist
- [x] Story 3.4: Enforce Local Socket Permission Model on Agent Connections
- [x] Story 3.5: Redact Secrets Across Identity, Capability, and Delivery Surfaces
- [x] Story 3.6: Canonicalize Agent Identity Across Multiple Host Connections
- [x] Story 3.7: Connect MCP Hosts Through `zornmesh stdio --as-agent`
- [x] Story 3.8: Degrade Gracefully for Baseline MCP Capability Limits
### Forensic Persistence, Trace, and Recovery
> Goal: Developers can reconstruct, inspect, tail, replay, and recover multi-agent conversations from durable local evidence when something breaks.

- [x] Story 4.1: Persist Envelopes, Audit Entries, and Trace Indexes
- [x] Story 4.2: Propagate Tracecontext and Emit OpenTelemetry Schema
- [x] Story 4.3: Capture Dead Letters with Structured Failure Reasons
- [x] Story 4.4: Inspect Persistence State with Structured Filters
- [x] Story 4.5: Reconstruct Conversation Timeline by Correlation ID
- [x] Story 4.6: Reconstruct Span Trees for Request/Reply and Streaming
- [x] Story 4.7: Live-Tail Envelopes by Subject Pattern
- [x] Story 4.8: Redeliver Previously Sent Envelopes Safely
- [x] Story 4.9: Configure Retention and Surface Retention Gaps
- [x] Story 4.10: Verify Audit Log Tamper Evidence Offline
### Compliance, Audit, and Release Trust Evidence
> Goal: Operators and compliance reviewers can verify release integrity, export evidence, prove audit-log integrity, handle redaction/deletion, and map events to required AI-risk/compliance frameworks.

- [x] Story 5.1: Produce and Verify Release Signatures, SBOMs, and Reproducibility Evidence
- [x] Story 5.2: Enforce Compliance Traceability Fields on Envelopes
- [x] Story 5.3: Export Evidence Bundle for a Time Window
- [x] Story 5.4: Redact Personal Data While Preserving Audit Integrity
- [x] Story 5.5: Map Envelopes to NIST AI RMF Functions and Categories
### Local Web Control Room and Safe Intervention
> Goal: Developers can open the local UI, observe connected agents, inspect trace chronology, send direct/broadcast messages safely, confirm outcomes, reconnect/backfill, and copy CLI handoffs. **Dependency gate:** Epic 6 is not implementation-ready, and Stories 6.2-6.9 must not begin, until Story 6.1 (a) verifies and references — by section — the existing v0.1 local UI architecture amendment that already supersedes earlier no-GUI/frontend/static-asset text, (b) pins the local UI framework wording so no Node-served runtime, hosted serving model, or remote-asset dependency can enter v0.1, and (c) scaffolds the local web app shell, shared UI/API taxonomies, component fixture baseline, and scope-boundary checks against that referenced architecture.

- [x] Story 6.1: Verify Local UI Architecture, Pin Framework Wording, and Scaffold Local Web App Shell
- [x] Story 6.2: Launch Protected Loopback UI with Offline Assets
- [x] Story 6.3: Render Live Agent Roster and Local Trust Status
- [x] Story 6.4: Render Daemon-Sequence Timeline and Event Detail Panel
- [x] Story 6.5: Open Focused Trace Reader with CLI Handoff Commands
- [x] Story 6.6: Send Safe Direct Messages from the UI
- [x] Story 6.7: Confirm Broadcast Scope and Show Per-Recipient Outcomes
  > As a developer
  > I want broadcasts to require explicit scope review and show per-recipient outcomes
  > So that broad-impact sends are deliberate and failures are visible for each target.
  > AC: Given the developer chooses broadcast mode, When recipients are selected or resolved from capability/status filters, Then the UI shows recipient count, included recipients, excluded or incompatible recipients, payload summary, capability warnings, and unsafe-scope warnings, And broadcast mode is visually and textually distinct from direct mode.
  > AC: Given a broadcast would affect multiple recipients, When the developer attempts to send, Then a confirmation dialog names the scope, recipient count, excluded/incompatible recipients, and payload summary, And the final confirmation requires explicit user action and avoids modal chains.
  > AC: Given recipient membership, capability compatibility, or agent status changes after the confirmation preview is shown, When the developer confirms the broadcast, Then the daemon validates the confirmed recipient snapshot/revision before accepting the send, And any recipient-set drift blocks the send, refreshes the preview, and requires explicit reconfirmation.
  > AC: Given the confirmed broadcast is sent, When delivery outcomes arrive, Then the UI displays a persistent per-recipient outcome list with queued, delivered, acknowledged, rejected, timed-out, dead-lettered, pending, stale, partial-success, all-failed, and success states as applicable, And failure reasons, timing, retry affordance, and inspect affordance are available per recipient where safe.
  > AC: Given a valid broadcast targets three recipients, When the daemon accepts the send, Then terminal per-recipient outcomes are displayed within 5 seconds unless explicit agent timeout policy exceeds that budget, And pending or partial states remain visible until every recipient reaches a terminal or policy-defined timeout state.
  > AC: Given the broadcast is accepted by the daemon, When audit evidence is persisted, Then the record links actor/session, requested recipient scope, previewed recipient snapshot, accepted recipient snapshot/revision, actual recipient list, excluded recipients, drift/reconfirmation outcome, trace/correlation ID, payload summary, and per-recipient delivery outcomes, And partial failure remains visible in both UI and audit evidence.
  > AC: Given broadcast tests run, When successful broadcast, partial failure, all failed, excluded recipients, incompatible recipients, stale recipients, unsafe scope, confirmation cancel, recipient drift after preview, reconfirmation required, stale snapshot rejected, duplicate submit, retry/inspect affordance, and audit-link scenarios execute, Then broadcast confirmation and per-recipient outcome behavior are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-7
- [x] Story 6.8: Reconnect, Backfill, and Preserve UI Context
  > As a developer
  > I want the UI to recover after refreshes, reconnects, and daemon restarts
  > So that I can continue investigating the same trace without losing chronology or mistaking gaps for complete evidence.
  > AC: Given the developer refreshes the browser or the UI reconnects after a transient disconnect, When the session is still valid, Then the UI restores the selected trace, selected agent where possible, active filters, and current view mode after daemon backfill completes, And restored events remain ordered by daemon sequence.
  > AC: Given the daemon restarts, becomes unavailable, reconnects, or reports a schema/session change, When the UI detects the transition, Then persistent status chrome shows starting, reconnecting, degraded, unavailable, stale, schema-mismatch, or session-expired state as applicable, And actions that cannot safely run are disabled with explanatory text.
  > AC: Given backfill includes retained, purged, late, duplicated, or reconstructed events, When the UI merges live and backfilled data, Then it deduplicates by stable daemon/event identity, marks retention gaps and late/reconstructed events, and preserves selected detail stability, And the UI never reorders trace events by browser receipt time.
  > AC: Given reconnect/backfill would exceed browser capacity, When the UI restores context, Then it restores the selected trace as a daemon-sequence window around the prior selection and marks the view partial until additional pages load, And actions and status copy do not imply the entire trace is loaded.
  > AC: Given reconnect or backfill cannot complete, When the developer views the affected trace or roster state, Then the UI shows an evidence-gap or recovery panel with safe next actions such as retry reconnect, inspect trace by CLI, inspect daemon health, or export audit evidence, And partial state remains visible rather than being replaced by a generic failure page.
  > AC: Given reconnect/backfill tests run, When browser refresh, transient disconnect, daemon restart, unavailable daemon, schema mismatch, expired session, retained plus purged data, late event, duplicate event, selected-detail stability, and failed backfill scenarios execute, Then reconnect behavior and context preservation are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-8
- [x] Story 6.9: Prove Accessibility, Responsive Behavior, and Browser Fixture Coverage
  > As a developer or QA reviewer
  > I want the local web UI's critical journeys verified across accessibility, responsive layouts, and supported browsers
  > So that the control room remains usable and trustworthy for real debugging sessions.
  > AC: Given the local UI critical journeys exist for roster inspection, trace timeline navigation, trace detail, focused trace reader, direct send, broadcast confirmation, outcome review, reconnect/backfill, and CLI command copy, When accessibility checks run, Then automated checks, keyboard-only walkthroughs, focus-order checks, screen-reader spot checks, reduced-motion checks, color-blindness checks, and no-color-only verification pass for those journeys, And failures identify the affected journey and component state.
  > AC: Given the UI runs at mobile, tablet, desktop, and wide desktop breakpoints, When responsive fixture tests execute, Then the layout uses one-pane mobile, two-pane tablet, and three-pane desktop behavior as specified, And no responsive mode reorders trace events, hides delivery/failure state, or loses selected detail stability.
  > AC: Given technical data contains long agent IDs, correlation IDs, subjects, timestamps, payload summaries, CLI commands, and error messages, When the UI renders across supported breakpoints, Then text remains readable, copyable where appropriate, and does not break timeline chronology, controls, or status visibility, And truncation or wrapping preserves accessible labels.
  > AC: Given browser E2E coverage runs for supported local browsers, When Chromium, Firefox, and WebKit/Safari-compatible scenarios execute, Then fixture-backed journeys for complete, partial, missing, reconstructed, and live traces; stale/disconnected agents; daemon unavailable; session expired; direct send; broadcast success; broadcast partial failure; validation-blocked send; late event arrival; and backfill are covered, And browser differences are represented as explicit unsupported-state or defect evidence rather than ignored.
  > AC: Given browser fixture evidence is collected for current stable Chromium, Firefox, and Safari/WebKit, When a browser-specific behavior differs or fails, Then the fixture records explicit unsupported-state or defect evidence, And cross-browser gaps cannot be silently ignored in release readiness.
  > AC: Given release/readiness checks include UI quality gates, When the UI test suite completes, Then accessibility, responsive, browser, offline-asset, and critical-journey fixture results are emitted as stable evidence for implementation readiness, And the suite fails explicitly when required browser or accessibility tooling is unavailable.
  > Spec: specs/planning-artifacts/epics.md#story-6-9

## Completed

## Notes
- Follow TDD methodology (red-green-refactor)
- One story per Ralph loop iteration
- Update this file after completing each story
