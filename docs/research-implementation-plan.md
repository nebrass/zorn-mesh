# Zorn Mesh research analysis and implementation plan

## Source note

Issue #2 references two research reports that are now available on the `main`
branch:

- [GeminiReport.pdf](https://github.com/nebrass/zorn-mesh/blob/main/GeminiReport.pdf)
- [Perplexity.pdf](https://github.com/nebrass/zorn-mesh/blob/main/Perplexity.pdf)

This plan synthesizes those reports into an actionable implementation direction
for Zorn Mesh.

## Product direction

Zorn Mesh should be a local-first agent message bus and control plane for
autonomous coding agents: a single-machine runtime that lets heterogeneous
agents discover each other, exchange authenticated messages, delegate tasks,
share state, and produce an auditable record without requiring a cloud broker.

## Research takeaways

1. **Zorn Mesh should be a local daemon, not a full service mesh.** The
   Perplexity report positions the product as a Dapr-style local application
   runtime plus NATS-like messaging core, focused on single-host agent
   coordination rather than L4 proxying, scheduling, or workflow planning.
2. **Use layered interoperability.** Gemini emphasizes MCP for agent-to-tool
   access and A2A for agent-to-agent delegation. Zorn Mesh should define its
   own canonical local envelope while providing MCP and A2A adapters at the
   edges.
3. **Make Unix domain sockets and JSON-RPC the default transport.** Perplexity
   recommends a Unix domain socket JSON-RPC 2.0 API for the daemon, with an
   optional localhost HTTP/WebSocket gateway for dashboards and runtimes that
   cannot use UDS directly.
4. **Persist the mesh in SQLite.** Both reports support local-first persistence:
   SQLite should hold the agent registry, capability catalog, append-only
   message log, audit events, dead letters, and replay metadata.
5. **Prefer practical local security first.** The MVP should use UDS file
   permissions, OS process identity, registration tokens, default-deny roles,
   and audit logging before adding stronger per-agent signatures or hardware
   attestation.
6. **Observability is a product feature.** Gemini highlights OpenTelemetry-style
   traces for non-deterministic agent flows; Perplexity recommends CLI and
   dashboard inspection for messages, dead letters, replay, and correlations.

## Proposed product architecture

### Core runtime

- Rust daemon named `zorn-meshd` as the single local control-plane entry point.
- Primary UDS JSON-RPC 2.0 API for agent registration, heartbeat, discovery,
  send, subscribe, acknowledgment, and admin operations.
- Optional loopback HTTP/WebSocket gateway for browser dashboards and non-UDS
  environments.
- Hybrid message/event bus supporting request/response, direct messages,
  pub/sub topics, streaming events, task results, and cancellation.
- SQLite-backed registry and append-only log for agents, capabilities, message
  metadata, task state, audit events, dead letters, and replay.

### Protocol and schemas

- Define a transport-agnostic canonical envelope with fields for
  `envelope_version`, `message_id`, `parent_id`, `correlation_id`, `trace_id`,
  `span_id`, `timestamp`, `from_agent`, `to`, `routing`, `identity`,
  `capability`, `reliability`, `security`, `payload`, `errors`, and `meta`.
- Use JSON-RPC framing and JSON payloads for the MVP wire protocol.
- Define JSON Schema or Protobuf contracts for envelope, agent registration,
  capability declarations, task lifecycle, and error responses.
- Model routing patterns as `request`, `response`, `event`, `stream`, `ack`, and
  `nack`.
- Model task lifecycle states as `queued`, `accepted`, `running`, `blocked`,
  `completed`, `failed`, and `cancelled`.

### Reliability model

- Support at-least-once delivery for application messages with idempotency keys,
  bounded retries, retry backoff, message expiry, and dead-letter queues.
- Allow at-most-once delivery for telemetry or low-value events.
- Track correlation IDs and parent IDs across multi-agent chains.
- Add bounded per-agent queues and backpressure so slow or crashed agents do not
  exhaust local resources.
- Provide replay tooling for debugging rather than promising exactly-once
  semantics in the MVP.

### Open-source bus comparison

Zorn Mesh should not reject existing messaging buses. The question is whether an
open-source bus should be the core of the MVP or an optional backend behind the
Zorn Mesh protocol.

| Approach | Strengths | Tradeoffs | Fit for Zorn Mesh |
| --- | --- | --- | --- |
| Custom local bus in `zorn-meshd` with SQLite | Small install footprint, direct UDS/OS-identity integration, agent-specific envelope, first-class audit/replay, no extra daemon to configure | Zorn Mesh owns routing, retries, backpressure, dead letters, and protocol hardening | Best MVP default because the product needs local agent identity, policy, and observability more than broker feature depth |
| NATS or NATS JetStream | Mature pub/sub and request/reply, strong performance, persistence via JetStream, clear path to later multi-host topologies | Adds another runtime and operational surface; agent registration, capability policy, JSON-RPC control plane, and audit semantics still need to be built around it | Strong candidate for a pluggable backend after the canonical envelope and local control plane are stable |
| Redis Streams or Redis pub/sub | Widely available, simple streams and consumer groups, easy local experimentation | Requires Redis, weaker fit for UDS process identity and durable audit semantics, pub/sub alone does not cover task lifecycle or policy | Useful for prototypes or adapters, but not the safest default for a local-first security boundary |
| RabbitMQ or Kafka | Rich broker ecosystems and durable messaging patterns | Heavyweight for a single-developer machine and optimized for service/backend systems rather than local agent IPC | Better as enterprise integration targets than as the default local mesh runtime |
| Dapr sidecars | Mature application-runtime model with pluggable pub/sub and service invocation | Requires sidecar/runtime conventions that are broader than Zorn Mesh's agent-focused control plane | Conceptual inspiration, but likely too broad for the first local-first implementation |

The recommended path is therefore hybrid: implement Zorn Mesh's canonical
envelope, agent registry, policy, UDS JSON-RPC API, and SQLite audit trail in the
MVP, but keep the routing layer abstract enough to add NATS JetStream or another
broker as an optional backend later. That preserves a small local-first default
while avoiding a dead end if users need mature broker features or multi-host
messaging.

### Security model

- Default to local-only UDS communication with strict socket permissions.
- Capture OS process identity metadata where available, including UID, GID, PID,
  process path, and trust level.
- Require explicit agent registration and issue per-agent registration tokens.
- Apply default-deny role and capability policies for high-risk actions such as
  filesystem, shell, and network access.
- Log registration, authorization, routing, and high-privilege capability
  decisions to the audit log.
- Add optional signed envelopes, nonce checks, anti-replay windows, per-agent
  keys, and stronger workload identity in later hardening phases.

### Interoperability

- Keep Zorn Mesh protocol-neutral at the core while aligning field names and
  error handling with JSON-RPC, MCP, A2A, and ACP where practical.
- Provide an MCP bridge so mesh actions can be exposed as tools and MCP servers
  can be represented as mesh capabilities.
- Provide an A2A-compatible adapter once the core envelope and task lifecycle are
  stable.
- Treat adapters as edge integrations; they should not replace the canonical
  Zorn Mesh envelope.

## Implementation roadmap

### Phase 0: Foundation

- Create the monorepo structure: `/core`, `/protocol`, `/sdk/ts`,
  `/sdk/python`, `/cli`, `/dashboard`, `/examples`, `/tests`, and `/docs`.
- Add Rust workspace tooling, formatting, linting, and test commands.
- Define the canonical envelope, registration, capability, task, and error
  schemas.
- Document local data paths, configuration, threat model, and MVP non-goals.

### Phase 1: Local single-host mesh

- Implement `zorn-meshd` with UDS JSON-RPC endpoints for registration,
  heartbeat, discovery, send, subscribe, acknowledgment, and admin inspection.
- Implement SQLite persistence for agents, capabilities, messages, tasks, audit
  events, and dead letters.
- Implement CLI commands such as `zorn daemon start`, `zorn agents`,
  `zorn capabilities`, `zorn send`, `zorn tail`, `zorn traces`, `zorn dlq`, and
  `zorn replay`.
- Add integration tests for enrollment, direct messaging, topic broadcast,
  request/response, task assignment, acknowledgment, cancellation, and replay.

### Phase 2: SDKs, examples, and adapters

- Ship TypeScript and Python SDKs for registering agents, subscribing to
  messages, sending requests and events, streaming updates, and sending
  acknowledgments.
- Add example agents for an IDE-style agent, a CLI linter/test runner, and an
  MCP bridge.
- Add the initial MCP bridge adapter.
- Define adapter contract tests so future A2A or ACP bridges can be validated
  against the same mesh behavior.

### Phase 3: Reliability, observability, and policy

- Add retries, backoff, dead-letter replay, message expiry, bounded queues, and
  slow-consumer handling.
- Add structured logs, trace IDs, span IDs, correlation views, and an optional
  OpenTelemetry export path.
- Add a minimal dashboard for agent status, topology, live traffic, task
  timelines, dead letters, and audit events.
- Add policy files for capability grants and workspace-level guardrails.

### Phase 4: Hardening and multi-host preview

- Add optional per-agent key pairs, signed envelopes, nonce validation, and
  anti-replay windows.
- Add fuzz, property, load, and chaos tests for malformed envelopes, crashed
  agents, slow consumers, duplicate deliveries, and retry loops.
- Add optional LAN peer discovery only after the single-host workflow is stable.
- Require encrypted transport and explicit peer trust before exchanging messages
  outside the local host.

## First milestone acceptance criteria

- A developer can start `zorn-meshd` locally and enroll two local agents.
- Agent A can discover Agent B's capabilities and send a signed or token-backed
  task request over UDS JSON-RPC.
- Agent B can accept, update, complete, fail, or cancel the task.
- The daemon persists agents, capabilities, messages, tasks, and audit events in
  SQLite.
- The CLI can display agent status, the full task timeline, dead letters, replay
  history, and the message audit trail.
- The mesh runs without cloud services and stores all state locally.
- Tests cover schema validation, enrollment, delivery, acknowledgment,
  idempotency, dead-letter routing, replay, and task lifecycle transitions.

## Open questions

- Should the MVP commit to Rust immediately, or start with a smaller prototype
  before introducing the Rust workspace?
- Should Protobuf be included in v0.1, or should JSON Schema be the only initial
  schema source of truth?
- Which SDK should ship first if TypeScript and Python cannot both be completed
  in the first milestone?
- Which coding-agent integration should be the launch example?
- Which observability data should be redacted by default to avoid storing
  sensitive prompts or source code in local traces?
