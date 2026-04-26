# Zorn Mesh research analysis and implementation plan

## Source note

Issue #2 references two research reports:

- [GeminiReport.pdf](https://github.com/user-attachments/files/27101518/GeminiReport.pdf)
- [Perplexity.pdf](https://github.com/user-attachments/files/27101528/Perplexity.pdf)

Both attachment URLs returned `404` during implementation, so this plan captures the
actionable direction from the issue brief, the current README product statement, and
the overlapping market themes for secure local coordination of autonomous coding
agents. Replace or refine the findings below once the report text is available.

## Product direction

Zorn Mesh should be a local-first coordination layer for autonomous coding agents:
a small runtime that lets agents discover each other, exchange authenticated
messages, delegate tasks, share state, and produce an auditable record without
requiring a cloud broker.

## Market and technology takeaways

1. **Agent protocols are becoming layered, not monolithic.** MCP is strongest for
   agent-to-tool and context access, while A2A-style protocols focus on
   agent-to-agent task exchange. Zorn Mesh should interoperate with these instead
   of trying to replace them.
2. **Local-first security is the differentiator.** Most multi-agent frameworks
   solve orchestration, but developer teams still need private, workstation-local
   or LAN-local messaging with explicit trust, encrypted transport, scoped
   capabilities, and inspectable logs.
3. **Frameworks need a neutral coordination substrate.** LangGraph, AutoGen,
   CrewAI, and similar systems can define agent behavior, but Zorn Mesh can own
   local discovery, identity, mailbox semantics, delivery guarantees, and
   cross-framework adapters.
4. **Adoption depends on developer ergonomics.** The first release should be easy
   to run from a CLI, observable by default, and embeddable through a small SDK
   before expanding into distributed or enterprise features.

## Proposed product architecture

### Core runtime

- Local mesh daemon with a Unix socket or localhost HTTP/WebSocket API.
- Agent registry with stable local identities, capabilities, status, and
  heartbeat timestamps.
- Message bus supporting direct messages, broadcast topics, task requests,
  task results, and cancellation.
- Durable local store for message history, task state, audit events, and agent
  metadata.

### Security model

- Generate a local mesh identity on first run.
- Require per-agent enrollment with scoped permissions.
- Sign messages with agent identity keys and reject unsigned or unknown-agent
  traffic.
- Encrypt non-loopback transports before supporting LAN or remote peers.
- Keep all secrets and private keys in OS-local storage or a clearly documented
  local data directory.

### Coordination semantics

- Define a small, stable envelope:
  `id`, `type`, `from`, `to`, `conversationId`, `createdAt`, `expiresAt`,
  `capabilities`, and `payload`.
- Support at-least-once local delivery with acknowledgements and idempotency keys.
- Model task lifecycle states: `queued`, `accepted`, `running`, `blocked`,
  `completed`, `failed`, and `cancelled`.
- Add leases for exclusive work claims so multiple coding agents do not edit or
  test the same scope accidentally.

### Interoperability

- Provide an MCP adapter so agents can expose mesh actions as tools.
- Provide an A2A-compatible adapter once the core envelope and task lifecycle are
  stable.
- Keep adapters thin: core Zorn Mesh concepts should remain protocol-neutral.

## Implementation roadmap

### Phase 0: Foundation

- Create the repository structure for runtime, CLI, SDK, tests, and docs.
- Add automated build, test, lint, and type-check commands.
- Define the message envelope and task lifecycle schemas.
- Document local data paths, configuration, and threat model.

### Phase 1: Local single-host mesh

- Implement the local daemon and CLI commands:
  `mesh init`, `mesh agent enroll`, `mesh agent list`, `mesh send`,
  `mesh task create`, `mesh task watch`, and `mesh logs`.
- Implement local identity creation, enrollment, message signing, and permission
  checks.
- Persist messages and task state locally.
- Add integration tests for enrollment, direct messaging, topic broadcast, task
  assignment, acknowledgement, and cancellation.

### Phase 2: Developer SDK and adapters

- Ship a TypeScript SDK for registering an agent, subscribing to messages,
  sending task updates, and acknowledging work.
- Add an MCP server adapter exposing mesh operations to compatible coding agents.
- Add examples for two local agents coordinating code review and test execution.

### Phase 3: Reliability and observability

- Add retries, dead-letter queues, message expiry, and idempotent task handling.
- Add structured logs, trace IDs, and a CLI timeline view for debugging agent
  interactions.
- Add policy files for capability grants and workspace-level guardrails.

### Phase 4: Multi-host preview

- Add optional LAN peer discovery.
- Require encrypted transport and explicit peer trust before exchanging messages.
- Add conflict and lease handling for agents operating across shared repositories.

## First milestone acceptance criteria

- A developer can start a local mesh daemon and enroll two local agents.
- Agent A can send a signed task request to Agent B.
- Agent B can accept, update, complete, or fail the task.
- The CLI can display the full task timeline and message audit trail.
- The mesh runs without cloud services and stores all state locally.
- Tests cover the message envelope, enrollment, delivery, acknowledgement, and
  task lifecycle transitions.

## Open questions

- Which language/runtime should be the initial implementation target?
- Should the first transport be Unix sockets, localhost HTTP/WebSocket, or both?
- What durability level is required for v1: append-only log, SQLite, or another
  embedded store?
- Which coding-agent integration should be the launch example?
- Should LAN support be postponed until after the local single-host workflow is
  stable?
