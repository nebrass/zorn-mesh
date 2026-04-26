---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary']
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
**Status:** In progress (Step 2c of 13 complete)

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
