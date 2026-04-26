---
project_name: 'zorn-mesh'
user_name: 'Nebrass'
date: '2026-04-26'
sections_completed: ['technology_stack', 'protocol_envelope_discipline', 'language_specific_rules', 'persistence_reliability', 'security_model', 'observability', 'testing_discipline', 'workflow_style']
existing_patterns_found: 0
source_inputs: ['GeminiReport.pdf', 'Perplexity.pdf']
status: 'complete'
optimized_for_llm: true
forbidden_pattern_count: 108
section_count: 109
greenfield: true
---

# Project Context for AI Agents — Zorn Mesh

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

> **State note:** Zorn Mesh is greenfield as of 2026-04-26 — no source code yet. Rules below are **prescriptive** (derived from the architecture briefs in `GeminiReport.pdf` and `Perplexity.pdf`, plus a multi-perspective review session that surveyed prior art across X11, D-Bus, Docker, VS Code LSP, Tauri, Litestream, MCP, and A2A). When code lands, update entries that prove inaccurate; do not silently drift from these rules.

---

## Technology Stack & Versions

### Status

No lockfiles exist yet. Versions specify **selection policy**, not pinned versions. When `Cargo.toml`, `package.json` (Bun-managed), and `pyproject.toml` land, replace any "TBD" with the pinned version. **Every rule below is a release-gating requirement.** No phased rollout, no "v2" deferral list, no "later hardening." If a rule is here, it ships.

### Architectural Shape (load-bearing — read this first)

Zorn Mesh is **a library that publishes an invisible co-located router daemon**. Agents import the library, call `connect()`, and exchange messages. The library is responsible for finding the daemon (well-known UDS path), auto-spawning it if absent, retrying the connect, and hiding the daemon's existence from the agent author. Agents never invoke the daemon binary directly, never write a config file to point at it, and never manage its lifecycle.

This is the **X11 / Docker socket / VS Code language client / Tauri sidecar / Litestream** pattern, applied to agent messaging. It is the empirical attractor for "multiple processes on one machine need to find each other and exchange messages with no operator." The architecture is not novel; it is the assembly of patterns each independently validated in production.

The daemon (`zorn-meshd`) is a **thin router** (dbus-broker rewrite lesson): its only jobs are accepting connections, maintaining a routing table, and delivering messages. Policy, persistence, and agent lifecycle are separate concerns handled by adjacent components — they do not live on the message hot path.

### Rendezvous Path

The daemon publishes its UDS at a deterministic, OS-resolved path. The library uses `try_connect → spawn_daemon_if_absent → retry_connect` (Tauri sidecar pattern). Override via `ZORN_SOCKET_PATH` env var.

- **Linux:** `${XDG_RUNTIME_DIR}/zorn-mesh/zorn.sock` (fallback `/run/user/$UID/zorn-mesh/zorn.sock`)
- **macOS:** `${TMPDIR}/zorn-mesh.sock` (the 104-byte UDS path limit forces `TMPDIR` over `~/Library/Application Support`)
- **WSL:** treated as Linux; the daemon and all agents must live in the same WSL distribution (cross-distro and Windows-host UDS bridging is out of scope)

The daemon emits its resolved path on startup as a parseable line on stdout: `zorn: socket=<path>`.

### Renamed Constraint (replaces "no brokers")

The exclusion is no longer "no message brokers." It is: **no separately-managed external runtime dependency.** This is the precise constraint the team actually wanted. It permits:

- An auto-spawned co-located daemon owned by the library (Zorn's `zorn-meshd`)
- Embedded library-only operation when the daemon is unreachable and a fallback transport is available

It prohibits:

- NATS / Redis / Kafka / RabbitMQ as a sidecar the user must install, start, or update
- gRPC as the agent-facing broker (no auto-spawn convention; clients always assume server is running)
- D-Bus (Linux-only; XML-IDL coupling)
- Any architecture where the user must edit a config file or start a process before the first message flows

### Bootstrap & Capability Negotiation — adopt MCP's handshake verbatim

The `connect()` call performs a JSON-RPC 2.0 `initialize` handshake **byte-compatible with the MCP 2025-03-26 spec**, extended with a single optional field for mesh-specific capabilities:

```json
{
  "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": {
    "protocolVersion": "2025-03-26",
    "capabilities": { /* MCP-standard capabilities */ },
    "clientInfo": { "name": "...", "version": "..." },
    "meshCapabilities": {
      "publish": true, "subscribe": true, "broadcast": true, "stream": true,
      "envelopeVersion": "1.0.0"
    }
  }
}
```

**Why this is load-bearing:** Claude Desktop, Cursor, VS Code Copilot, and the Gemini agent surfaces all already speak MCP `initialize`. An agent built for any of them can register with `zorn-meshd` with zero modification beyond pointing at the UDS. This directly delivers the "first message in 10 minutes" job.

### Identity Primitive — wrap A2A AgentCard

Agent identity in the canonical envelope uses Google's A2A v0.2+ `AgentCard` document verbatim as the identity payload. The daemon's in-memory agent registry is keyed by `AgentCard.id`. Capability advertisement reuses A2A's capability descriptor structure rather than inventing a new IDL.

**Why this is load-bearing:** A2A is Linux Foundation governed, has 150+ organizational adopters, and was designed specifically for peer-to-peer agent addressing. Wrapping it gives Zorn Mesh interop with the broader A2A ecosystem at zero cost. Inventing a parallel identity model would create permanent adapter debt.

### Wire Format & Envelope

- **Wire protocol:** JSON-RPC 2.0 over Unix Domain Socket (primary) and HTTP/WebSocket (gateway, for browser dashboards only).
- **Canonical envelope:** mandatory `zorn_envelope` JSON-RPC extension field carrying:
  - `agent_id` (A2A AgentCard.id)
  - `capability_ref` (A2A capability descriptor reference)
  - `trace_id`, `span_id`, `parent_id` (W3C tracecontext)
  - `correlation_id`
  - `envelope_version` (semver; daemon refuses incompatible majors with a registered error code)
  - `idempotency_key`
  - `delivery_mode` (`at_least_once` | `at_most_once`)
  - `ttl_ms`
- **JSON-RPC error code registry:** committed at `/docs/jsonrpc-error-codes.md`. Every daemon and SDK error code draws from this registry. CI rejects PRs that introduce a code not listed.
- **Wire-protocol version negotiation:** the MCP `initialize` handshake doubles as the version handshake; daemon and client exchange supported `envelopeVersion` ranges.

### Schema Layers

- **Capability I/O contracts:** JSON Schema 2020-12, **emitted from TypeBox** at SDK build time, persisted in the agent registry. (TypeBox locked over Zod: native draft 2020-12 emission with no translation layer.)
- **Canonical internal model (envelope, registry rows, audit entries):** Protocol Buffers, **proto3 with the `optional` keyword required on every presence-sensitive field**. Generated bindings: `prost` (Rust), `@bufbuild/protobuf` (TS, Bun-compatible), `betterproto` (Python).
- **Schema source-of-truth direction:** TypeBox → JSON Schema 2020-12 (capabilities); `.proto` files → generated bindings (envelope, registry, audit). The two layers do not generate each other; they meet at the daemon's UDS ingress.
- **Schema enforcement gate (load-bearing):** the daemon synchronously validates every inbound message at the UDS boundary against (a) JSON-RPC 2.0 envelope schema, (b) canonical envelope protobuf schema, and (c) the registered JSON Schema for the addressed capability. Validation failure → `nack` with a registered error code. **No silent passthrough.** This is the mechanical feedback loop that makes multi-layer schemas safe without an external schema registry.
- **Cross-language byte-equivalence corpus:** golden binary protobuf messages at `/conformance/protobuf-vectors/`; CI gate decodes the corpus in all three SDKs and asserts equivalence.
- **Versioning:** semver everywhere. `envelope_version`, `capability.version`, and `capability.schema_version` are independently semver'd.

### Core daemon — `zorn-meshd`

- **Language:** Rust, **stable channel only** (no nightly features in production code).
- **MSRV:** Latest stable minus 2 minor versions, **must include 1.85+** (required for edition 2024). CI gates MSRV via `cargo-msrv verify`.
- **Edition:** `2024`.
- **Async runtime:** `tokio` exclusively. No `async-std`, `smol`, or mixed runtimes.
- **Cargo profiles** (all explicitly defined in `Cargo.toml`):
  - `dev`: defaults (incremental, no LTO)
  - `test`: defaults + `opt-level = 1` for tractable async test runtimes
  - `release`: `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`
- **Routing discipline (dbus-broker lesson):** the message routing path holds no policy logic. Auth, rate limits, audit hooks, and ACL evaluation run on adjacent tokio tasks reading from observation channels — never inline in the routing hop.
- **Fault injection:** the daemon exposes an out-of-band debug control socket (`$RUNTIME_DIR/zorn-debug.sock`, disabled unless `ZORN_DEBUG=1`) for chaos/fault injection — stall, drop, delay, force-close. Required for chaos tests; ships in every build.
- **Auto-spawn invariant:** when invoked by the SDK's `try_connect` shim, the daemon must reach "accepting connections" within 200 ms cold start. The SDK's connect retry budget is 1 s with exponential backoff.

### TypeScript SDK + Dashboard runtime — Bun

- **Runtime:** **Bun** (latest stable, pinned in `package.json` `"packageManager": "bun@x.y.z"` and via `.bun-version`). No Node.js fallback path. No npm, pnpm, or yarn anywhere in the repo.
- **Why Bun over Node:** native TypeScript execution (no `tsc` build step for the SDK), integrated package manager + test runner + bundler, faster cold start for the dashboard dev server, single binary that the daemon's auto-spawn harness can invoke uniformly.
- **TypeScript config:** `strict: true`, `noUncheckedIndexedAccess: true`, `exactOptionalPropertyTypes: true`, `moduleResolution: "bundler"`, `module: "esnext"`. The `tsconfig.json` is a committed golden file; CI rejects PRs that mutate it without an accompanying ADR.
- **Module system:** ESM only.
- **Schema runtime:** **TypeBox**. Zod is prohibited.
- **Test runner:** Bun's built-in `bun test`. No Vitest, Jest, or Mocha.
- **Dashboard:** Next.js 15+ / React 19+, App Router, run under Bun (`bun --bun next dev`, `bun --bun next build`). Communication is WebSocket → localhost gateway only — never direct UDS. Optional at runtime; daemon must be fully usable without it.

### Python SDK — `zorn-mesh`

- **Runtime:** CPython **3.11, 3.12, and 3.13** — all three tested in CI matrix.
- **Type checking:** `mypy --strict` (chosen over pyright). Config at repo root.
- **Async:** `asyncio` is the primary event loop. `anyio` is permitted as a structured-concurrency abstraction layer (its `TaskGroup` provides cleaner cancellation propagation than bare `asyncio.create_task`). `trio`, `gevent`, and `eventlet` remain forbidden.
- **Dependency manager:** `uv`. `uv.lock` is the source of truth; CI runs `uv lock --check` on every PR.

### CLI — `zorn`

- Implemented in Rust, sharing crates with `zorn-meshd`.
- Distributed as a single static binary; no `bun`/`uv` wrapper.
- `zorn` invocations may also trigger the auto-spawn rendezvous (e.g., `zorn agents` connects via the SDK shim).

### Persistence

- **Engine:** SQLite, statically bundled (not system-installed).
- **Driver:** `sqlx` (async, compile-time-checked queries).
- **Mode:** WAL (`journal_mode=WAL`).
- **Sync mode (per-pool):** the writer pool uses `synchronous=FULL` (real fsync per commit, ~1 ms overhead on NVMe — required for the "ACK only after COMMIT" reliability invariant). The reader pool uses `synchronous=NORMAL` (read-only, no write-durability concern). Category 4 specifies the per-pool pragma policy.
- **Migrations:** `sqlx migrate`, forward-only, applied at daemon startup. Migration files at `/migrations/`, monotonically numbered. A migration test harness applies each migration to a blank DB, seeds known state at each version, and verifies daemon startup — required CI gate.
- **No other database engines.** No Postgres, no Redis, no LMDB.

### Observability

- **Standard:** OpenTelemetry — traces, metrics, logs.
- **Export:** OTLP (gRPC default, HTTP/protobuf fallback). gRPC permitted *only* for the OTLP export client.
- **Local-developer convenience:** the daemon exposes a `/metrics` Prometheus scrape endpoint on the gateway (loopback-only, behind the same CSRF token as the dashboard). Required, not optional — local-first observability cannot depend on running an OTel collector.
- **Trace propagation:** W3C tracecontext fields (`trace_id`, `span_id`, `parent_id`) are first-class envelope members; the daemon propagates them across every routing hop.
- **Canonical env vars** (committed to `.env.example`): `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES`, `ZORN_TRACE_SAMPLE_RATIO`.
- **Test collector:** in-tree OTel collector config (`/test-infra/otel-collector.yaml`) launched by the shared test harness; integration tests assert on span structure via this collector.
- No vendor-specific SDKs (Datadog, New Relic, Honeycomb agents) anywhere in the core.

### Test Infrastructure (load-bearing)

- **Shared daemon fixture:** crate `zorn-mesh-test-harness` exposes a launcher API (Rust trait + Bun and Python wrappers calling out to the same binary) that boots `zorn-meshd` against a temp UDS path with deterministic teardown. All three SDK test suites consume this — no SDK rolls its own daemon launcher.
- **Test vector corpus:** `/conformance/` directory contains `protobuf-vectors/`, `jsonrpc-vectors/`, `jsonschema-vectors/`, plus `mcp-handshake-vectors/` (golden MCP `initialize` exchanges) and `agentcard-vectors/` (golden A2A AgentCard documents).
- **Cross-SDK conformance runner:** single CI job (`conformance.yml`) executes the corpus against Rust, TypeScript (Bun), and Python in parallel and asserts byte/value equivalence.
- **MCP compatibility CI gate:** the daemon's `initialize` handshake is exercised against the upstream MCP reference test suite; regression is a release blocker.
- **Auto-spawn lifecycle tests:** verify cold-start latency budget (200 ms), concurrent-connect race (10 simultaneous SDK `connect()` calls produce one daemon, not ten), and PID-file / lockfile correctness.
- **Chaos tests:** drive the daemon's debug control socket. UDS reconnection, slow-consumer backpressure, mid-stream cancellation, DLQ overflow, and daemon-crash-during-routing scenarios are all required CI gates.

### Build / Repo Tooling

- **Repo strategy:** monorepo.
- **Top-level task runner:** `just` (single `justfile` at repo root). Invokes `cargo xtask` for Rust workflows, `bun run` for TS/dashboard workflows, `uv run` for Python workflows.
- **Lockfiles committed:** `Cargo.lock`, `bun.lockb`, `uv.lock`. CI fails on uncommitted lockfile drift.

### Documented for evaluation, not in scope at v1

These are precedents surfaced during the lateral-review session that have proven valuable in adjacent systems but require benchmark evidence before adoption. They are documented here so contributors do not propose them as v1 changes:

- **Shared-memory membership table** (PostgreSQL pattern) — faster than filesystem for "who's alive right now." Evaluate after the in-memory registry shows measurable read contention.
- **Watched-directory inbox per agent** (inotify/FSEvents) as inspection surface — `cat /tmp/zorn/agents/X/inbox/*` as a debug aid, not a primary transport.
- **Loopback multicast announcement** for daemon-less peer discovery — only relevant if a true daemonless mode is later required.

### Explicit Non-Dependencies (do not add)

- ❌ **Node.js, npm, pnpm, yarn** — Bun is the only JavaScript runtime and package manager.
- ❌ **NATS, Redis, Kafka, RabbitMQ** — operate as separately-managed runtime dependencies, which the renamed constraint prohibits.
- ❌ **D-Bus** — Linux-only, XML IDL coupling.
- ❌ **gRPC as a public/agent-facing transport** — no auto-spawn convention, schema-mismatch with MCP/A2A envelopes. Permitted only for the OTLP export client.
- ❌ **Docker as a runtime requirement** — daemon must run as a host binary. Docker permitted only for ephemeral CI test infrastructure.
- ❌ **Cloud SDKs** (AWS/GCP/Azure) in the core.
- ❌ **Vitest, Jest, Mocha, Vite** — Bun's built-ins replace them.
- ❌ **Zod** — TypeBox is the JSON Schema source layer.
- ❌ **pyright** — mypy is the type checker.
- ❌ **Async runtimes other than tokio (Rust) / asyncio+anyio (Python)** — no async-std, smol, trio, gevent, eventlet.
- ❌ **MCP host-as-hub topology** — Zorn Mesh adopts MCP's handshake byte-compatibly but rejects MCP's all-traffic-routes-through-host model. Agents address each other peer-to-peer via A2A AgentCard identity, not via a host hub.

---

## Protocol & Envelope Discipline

### Frame Discriminant (load-bearing)

Every byte sequence delivered across the UDS socket is a **frame**. The first byte (`frame_type`) discriminates the protocol tier. The daemon reads `frame_type` before invoking any validator.

| `frame_type` | Tier | Validation path | Size cap | Fragmentable |
|---|---|---|---|---|
| `0x01` | **Control frame** | Frozen daemon-owned schema, validated before any envelope path | 512 bytes total | No |
| `0x02` | **Envelope frame** | Three-layer validation (JSON-RPC + zorn_envelope + capability schema) | No hard cap | No (length-prefixed) |
| `0x03`–`0x0F` | Reserved | — | — | — |

This partition is mandated by production precedent (HTTP/2, WebSocket, AMQP). Without it, the validation gate creates a bootstrap deadlock when an ack must report a validation failure.

### Frame Wire Format

All frames use a **4-byte big-endian length prefix** followed by the 1-byte `frame_type` and the payload. Total wire layout:

```
[length:u32 BE][frame_type:u8][payload:bytes]
```

Length covers `frame_type + payload`. Length validation precedes type discrimination. Length > 16 MiB → daemon closes the connection with a registered protocol-violation control frame.

### Control Frames (frozen at v1)

Control frames carry transport-level signals. Their schema is owned entirely by the daemon, frozen at v1, and **not extensible through normal version negotiation** — only through a new major protocol version.

**Frozen opcode space (no additions permitted):**

| Opcode | Name | Purpose |
|---|---|---|
| `0x01` | `ACK` | Acknowledge an envelope or stream chunk |
| `0x02` | `NACK` | Reject with reason code (incl. validation failures) |
| `0x03` | `STREAM_END` | Terminate a stream |
| `0x04` | `PING` | Liveness probe |
| `0x05` | `PONG` | Liveness response |
| `0x06` | `CAPABILITY_PROBE` | Pre-handshake compatibility query |
| `0x07`–`0x0F` | Reserved | — |

**Control frame schema (CBOR, fixed grammar):**

- `opcode` (u8, required)
- `correlation_id` (UUIDv7, required for `ACK`/`NACK`/`STREAM_END`; null otherwise)
- `code` (u16, required for `NACK`; absent otherwise — references the JSON-RPC error code registry)
- `reason` (UTF-8 string ≤256 bytes, optional)
- `traceparent` (W3C tracecontext string, optional but **schema-mandated** — preserves span continuity through control flow)
- `tracestate` (W3C tracecontext string, optional)

Control frames are **exempt from idempotency dedup**, **exempt from TTL enforcement**, and **exempt from at-least-once persistence**. They are transport primitives, not application messages.

### The Three Layers (envelope frames only)

Every envelope frame (`frame_type = 0x02`) carries three nested contracts. Violating any one is a release-blocking bug.

1. **JSON-RPC 2.0** — outer wire framing (`jsonrpc`, `method`, `id`, `params` | `result` | `error`).
2. **`zorn_envelope`** — JSON-RPC extension field.
3. **Capability payload** — JSON Schema 2020-12-validated body.

The daemon validates all three synchronously at UDS ingress. SDKs MUST validate all three before transmit.

### Bootstrap Envelope Schema (resolves the handshake chicken-and-egg)

A *bootstrap envelope schema* is compiled into the daemon binary and loaded before the session is established. This schema is the minimal subset of `zorn_envelope` required for `initialize` and `agent.register` methods. It is versioned independently and cannot be extended through SDK negotiation.

- **`initialize`** is an **envelope frame**, not a control frame, validated against the bootstrap envelope schema.
- **`agent.register`** (carrying the A2A AgentCard) is an envelope frame validated against the bootstrap schema; identity claims traverse the validation gate.

### Capability-Probe Handshake (replaces hard major-version refusal)

The `CAPABILITY_PROBE` control frame is sent before any envelope. The daemon responds with a `PONG` carrying its supported `envelope_version` ranges and required capability bitmask. Negotiation rules:

- **Same major, sender minor ≤ daemon minor:** session proceeds.
- **Same major, sender minor > daemon minor:** session proceeds; sender SHOULD NOT use minor-only fields.
- **Different major:** daemon attempts a capability-probe; if the sender's mandatory capabilities are all present in the daemon's supported set, session proceeds with feature degradation.
- **Mandatory capability missing:** daemon refuses with `NACK` carrying a registered error code that names the specific missing capability — never just "version mismatch."

This pattern matches HTTP/2 ALPN fallback, PostgreSQL wire negotiation, and MCP additive versioning. Hard refusal on version number alone is forbidden.

### Envelope Field Discipline

- **snake_case** for every envelope field name. Match the canonical Protobuf schema exactly. JSON ↔ Protobuf round-trip MUST be bit-identical for the byte-equivalence test corpus.
- **Required fields** (presence enforced at ingress; missing → `NACK` with registered error code):
  - `agent_id` (A2A AgentCard.id — see Identity Stability below)
  - `envelope_version` (semver)
  - `message_id` (UUIDv7 generated **once per logical operation**, **stable across retries**)
  - `timestamp` (RFC3339 with `Z` suffix and **always 6 decimal places** of microsecond precision; e.g. `2026-04-26T15:59:55.294000Z`)
  - `routing.to`, `routing.pattern` (`request` | `response` | `event` | `stream`)
  - `tracing.traceparent`, `tracing.tracestate` (W3C tracecontext; SDK generates root if no parent)
- **Reliability fields** (required for `request` and durable `event`):
  - `idempotency_key` (UTF-8, ≤128 **bytes** — SDKs MUST validate by `byte length`, not character count)
  - `delivery_mode` (`at_least_once` | `at_most_once`)
  - `ttl_ms` (non-negative integer; `0` means "no TTL")
  - `attempt` (1-indexed; daemon increments on retry)
- **Identity block** (always present; field nullability platform-dependent — see Cross-Platform Identity below).

Note: `routing.pattern` no longer includes `ack` or `nack` — those are control_frame opcodes.

### Identity Stability — AgentCard.id

`AgentCard.id` is the scoping key for the daemon's idempotency dedup table. Stability is mandatory:

- AgentCard.id MUST be derived from durable, externally-configured input (a `ZORN_AGENT_ID` env var, a config file path, or a stable hash of `agent_name + machine_id`). It MUST NOT be derived from PID, process start time, or random seed.
- The daemon MUST reject `agent.register` calls that change AgentCard.id mid-session and emit a registered error code.
- Conformance test: restart an agent process; the daemon MUST treat the post-restart connection as the same identity if AgentCard.id matches.

### Idempotency — corrected formula

Idempotency keys scope to `(idempotency_key, to_agent_id, capability_ref_with_version)` tuples.

- **`capability_ref` MUST include the capability semver** (e.g., `code_edit@2.1.0`). Two structurally identical payloads addressed to different capability versions MUST NOT be deduplicated.
- **Dedupe window:** **5 seconds default** (configurable per capability via AgentCard, max 300 s). The previous 60 s default lacked local-bus evidence.
- **Synthetic key for `request` without explicit key:**
  `idempotency_key = sha256(message_id ‖ 0x1F ‖ agent_id ‖ 0x1F ‖ capability_ref_with_version)`
  where `‖` is byte concatenation and `0x1F` is the ASCII unit-separator. **Payload is excluded** (deterministic given the other three; including it breaks dedup under non-canonical map ordering).
- **`message_id` is generated once per logical operation** at the SDK call site and **reused across all retries**. SDKs MUST persist `message_id` in the retry loop's state.
- Dedup is **daemon-owned**. SDK-side caching is forbidden.
- Events without explicit key are NOT deduplicated.
- The daemon treats keys as **opaque bytes** — no parsing, no embedded-timestamp interpretation.

### Delivery Semantics

- **at_least_once** (default for `request`, `response`, durable `event`):
  - Daemon persists envelope to SQLite append-only log before delivery attempt.
  - On consumer ack-timeout or nack-with-retryable, daemon retries with exponential backoff (`max_retries`, `retry_backoff_ms` per capability).
  - Exhausted retries → message moved to `dead_letters` table with reason code; never silently dropped.
- **at_most_once** (telemetry-grade events only):
  - No persistence, no retry, no DLQ. Drop on backpressure.
  - SDKs MUST raise an error before transmit when an application requests `at_most_once` for `request`/`response` patterns. Enforcement: **type-state in Rust** (compile-time), **type+schema guard in TypeScript** (build + import time via TypeBox `.refine()`), **`@model_validator(mode='after')` in Python** (construction time, not import time).
- **Exactly-once is not implemented and SDKs MUST NOT advertise it.**

### TTL Base Clock and Skew Window

- **TTL anchor:** `ttl_ms` is measured from **daemon ingress wall clock**, not sender `timestamp`. The sender's clock is informational only.
- **Acceptable skew:** ±30 s default between sender `timestamp` and daemon ingress clock. Configurable via daemon flag. Beyond skew → daemon emits a warning span; >300 s → `NACK` with `clock_skew_excessive` error code.
- **TTL/retry interaction:** if a retry's scheduled delivery would exceed the original `ttl_ms` budget (computed from daemon ingress clock at first acceptance), the message is moved to DLQ instead of retried. Retries never deliver past TTL.

### Ack / Nack Protocol — control_frame tier

- For `request`: implicit `ACK` via the `response` envelope. Explicit `ACK` control frame only if response is delayed beyond `ttl_ms / 2` (long-running operation acknowledgement).
- For durable `event`: consumer SDK sends explicit `ACK` control frame after handler returns success. `NACK` control frame with retryable error code triggers retry; non-retryable code → DLQ.
- **Acks/nacks are control frames, never envelopes.** They reuse the same UDS socket but a distinct protocol tier.
- Ack timeout: `ttl_ms` if set, else daemon default (5 s). Exceeded → daemon treats as nack-retryable.

### Subject / Address Grammar

Agents are addressed via subject strings of shape `agent://<agent_id>/<capability_ref_with_version>` or `topic://<namespace>/<topic_name>`. Wildcards permitted only on subscribe:

- `*` matches a single segment
- `>` matches multiple trailing segments (terminal only)

Subjects are case-sensitive ASCII; permitted characters `[a-zA-Z0-9._@-]`. `@` is reserved for the version separator. Validation happens at SDK call site AND daemon ingress.

### Versioning Discipline

- **Major bump** on `envelope_version`: daemon attempts capability-probe; refuse only on missing mandatory capability with named error code (never on version-number alone).
- **Minor bump:** backward-compatible field addition. Receivers MUST set unknown-field handling to "preserve and pass through" — never strip. Tested by canary-field CI fixture (see Conformance Test Anchors).
- **Patch bump:** clarifications only; never affects wire bytes.
- **Capability versions** are independent of envelope versions.

### Streaming — byte-budget window

- Streams are envelope frames with `routing.pattern: "stream"`, correlated by `stream_id` (UUIDv7).
- Each chunk is a self-contained envelope frame.
- Stream end is signaled by a `STREAM_END` **control frame** carrying the originating `stream_id` in `correlation_id`.
- Cancellation: client sends `STREAM_END` control frame with non-zero `code` referencing the originating `stream_id`. Producer halts within one chunk boundary.
- **Backpressure budget:** **256 KiB outstanding-bytes window per stream** (configurable per capability). The previous 64-chunk cap was incompatible with token-streaming use cases (a 4000-token LLM response would saturate at chunk 64). Byte-budget windowing matches HTTP/2 (65 KB initial), gRPC (no chunk cap, byte-level flow control), and serves the primary use case.
- When the window is exhausted, the daemon emits a `NACK` control frame with `flow_control` reason; producer pauses generation.

### WebSocket Gateway Path — separate identity rules

The gateway translates between localhost WebSocket (browser dashboard) and UDS (daemon). Browsers cannot provide UDS `SO_PEERCRED` credentials, so identity rules differ:

- Envelope frames over WS gateway MUST carry `transport: "ws_gateway"` in `zorn_envelope`.
- For `transport = "ws_gateway"`, the identity block's `uid`/`gid`/`pid` are **informational only, never authorization input**.
- The gateway injects `gateway_verified: true|false` and a one-time session token bound to the WebSocket connection. Downstream routing decisions MUST use the session token, not self-reported uid/gid/pid.
- Schema gate at WS ingress: same envelope structure, but identity validation is relaxed (uid/gid/pid may be null regardless of `trust_level`).

### Cross-Platform Identity

- **Linux/macOS UDS:** `uid`, `gid`, `pid` always present (resolved via `getuid`/`getgid`/`getpid`).
- **Windows:** `uid` and `gid` are nullable (no POSIX equivalent); `pid` always present (`GetCurrentProcessId`). The schema permits null for `uid`/`gid` only when the runtime platform reports no POSIX identity.
- **Container without `/proc`:** SDKs MUST resolve `pid` via syscall, never via `/proc/self`.

### W3C tracecontext Conventions

- `traceparent` and `tracestate` are SDK-synthesized and always present in envelope frames.
- `parent_id` (within `traceparent`) MAY be the all-zero string for root spans — present-but-empty is distinct from absent.
- Control frames carry `traceparent`/`tracestate` when they participate in a flow with a known trace context (acks/nacks for traced envelopes); otherwise they carry the all-zero traceparent. Span continuity is preserved across the partition.

### SDK Translation Wall (load-bearing UX contract)

No `control_frame` vocabulary, opcode, or numeric code crosses the SDK boundary into user-facing API surfaces, exception messages, log lines, or default debug output. The SDK owns translation:

- `NACK` opcode + `code` → typed user-facing exception (`MessageRejectedError`, `SchemaMismatchError`, `FlowControlError`, `ClockSkewError`, etc.).
- `STREAM_END` control + non-zero `code` → typed stream cancellation event.
- `PING`/`PONG` → consumed entirely within the SDK; never surfaced.
- User-facing error messages MUST name the offending envelope field path, not the wire opcode.

Conformance test: scrape every published exception message, log line, and doc string across all three SDKs for the substrings `control_frame`, `opcode`, `0x0`, `nack code`. Any hit fails CI.

### Debug Output Format

When `ZORN_DEBUG=1`, SDKs MUST emit traffic in a tagged format:

```
[APP]  → lint.request {file: 'main.py'}
[APP]  ← lint.response {ok: true}
```

Control frames are hidden by default. `ZORN_DEBUG=verbose` adds:

```
[CTRL] ← ACK {correlation_id: ...}
[CTRL] ← NACK {code: 4, reason: "schema mismatch"}
```

Tag column width is fixed (`[APP]`, `[CTRL]`) so output remains alignable. Color is environment-dependent.

### Conformance Test Anchors (each rule has a named fixture)

Every "MUST"/"MUST NOT" rule above is anchored to a fixture under `/conformance/`. Rules without a named anchor are decoration. The anchors required at v1:

- `/conformance/frame-discriminator/` — adversarial corpus: malformed envelopes disguised as control frames, valid control frames with envelope-shaped payloads, length-prefix boundary mutations, oversized control frames (>512 bytes), fragmented control frames.
- `/conformance/idempotency/` — same logical operation across Rust/TS/Python SDKs produces byte-identical synthetic key; capability-version isolation; 5 s window expiry.
- `/conformance/handshake/` — capability-probe with matching/mismatching mandatory capabilities; major-version mismatch with compatible capabilities; bootstrap envelope schema rejection of non-bootstrap methods.
- `/conformance/agentcard-stability/` — restart-with-same-id, restart-with-changed-id, mid-session id change.
- `/conformance/timestamp/` — golden RFC3339 strings with explicit `Z` and 6 decimal places; clock-skew rejection at ±30 s and ±300 s.
- `/conformance/ttl-retry/` — retry that would exceed TTL goes to DLQ instead.
- `/conformance/streaming/` — 256 KiB window saturation triggers flow-control NACK; cancel during final chunk; concurrent multi-stream interleave.
- `/conformance/identity-platform/` — Linux/macOS uid/gid present; Windows uid/gid null; container `/proc`-free pid resolution.
- `/conformance/ws-gateway/` — informational identity, gateway_verified flag, session-token-required-for-authz.
- `/conformance/translation-wall/` — no SDK exception/log/doc string contains forbidden substrings (regex CI gate).
- `/conformance/canary-field/` — minor-version field added at ingress survives all egress paths.
- `/conformance/control-frame-traceparent/` — span continuity across NACK boundaries.

### Forbidden Patterns

- ❌ **Inline ack-batching that omits per-message ACK control frames.** Every ack is a discrete control frame.
- ❌ **Optional fields treated as "missing means default."** Proto3 `optional` keyword is mandatory on every presence-sensitive field.
- ❌ **SDK-side idempotency caching.** Dedup is the daemon's job.
- ❌ **JSON-RPC batch requests.** Daemons reject batch-encoded requests with a registered error code.
- ❌ **Out-of-band correlation channels.** All correlation flows through `tracing.traceparent`, `correlation_id`, and `parent_id`.
- ❌ **Envelope mutation during routing.** The daemon writes only to its own namespace partitions (`routing.daemon_*`, `tracing.*` — never to client-supplied fields). The client-vs-daemon partition is enforced by schema, not by prose.
- ❌ **Truncating or coercing fields silently.** Validation failure → `NACK` control frame with named code.
- ❌ **Hard-refuse on version-number alone.** Refusal must name a missing mandatory capability.
- ❌ **`control_frame` vocabulary in user-facing surfaces.** See SDK Translation Wall.
- ❌ **Fragmented control frames.** Single read, single parse.
- ❌ **Extending the control-frame opcode space without a major protocol bump.**
- ❌ **Treating sender `timestamp` as the TTL base.** Daemon ingress clock is the only TTL anchor.

---

## Language-Specific Rules

These rules prevent footguns that are idiomatic in one language but architecturally wrong here. They are additional to (not a replacement for) Categories 1–2.

### Rust (daemon, CLI, shared crates)

#### Error Handling

- **No `unwrap()` / `expect()` outside `tests/` and `xtask/`.** CI gates with `#![deny(clippy::unwrap_used, clippy::expect_used)]` at crate root; `#![allow(clippy::unwrap_used)]` permitted only inside `#[cfg(test)]` blocks. Genuine infallibility uses `unreachable!()` with an explanatory string.
- **No `panic!()` on the routing hot path.** Routing code returns `Result<_, RouteError>`; `RouteError` maps to a registered NACK code.
- **`thiserror` for crate errors, `anyhow` only in `xtask/` and integration tests.** Crate-public error types must be enumerable; `anyhow::Error` swallows variants.
- **Error types implement `std::error::Error` AND carry a `#[error]` source chain.** Tracing spans capture the chain via `tracing::error!(error = ?e)` — never `format!("{}", e)`.

#### Async Discipline

- **One `#[tokio::main]` in the binary entrypoint, never in libraries.** Library crates expose `async fn` and require the caller to provide the runtime.
- **`tokio::spawn` is forbidden in routing *functions*.** Background tasks must be encapsulated in dedicated actor types with an explicit `shutdown()` contract; `tokio::spawn` is permitted only inside the actor's `run()` method. JoinSet is the structured-concurrency primitive within request-handler scopes; **JoinSet handles must remain in scope of the owning component and never be passed across module boundaries.**
- **No `block_on` in async contexts.** Enforced via `clippy::disallowed_methods` listing `tokio::runtime::Runtime::block_on` and `tokio::runtime::Handle::block_on`.
- **`select!` arms must be cancel-safe.** Wrap non-cancel-safe operations (e.g., `BufReader::read_line` mid-line) in `tokio::pin!` + dedicated tasks.
- **Bound every channel.** `mpsc::channel(N)` not `mpsc::unbounded_channel()`. Unbounded channels turn backpressure into OOM.

#### Type Discipline

- **`#[forbid(unsafe_code)]` at every crate root** except platform-syscall crates (where `SO_PEERCRED`, `inotify`, `kqueue`, `FSEvents` wrappers live). Platform crates are individually reviewed: each `unsafe` block carries a `// SAFETY:` comment naming the invariant; CI counts and reports `unsafe` blocks per crate; a code-owner approval is required on any addition.
- **Newtype envelope-relevant primitives.** `AgentId(String)`, `MessageId(Uuid)`, `CapabilityRef(String)`, `IdempotencyKey(Bytes)` — never raw `String`/`Uuid`. Prevents argument-order bugs.
- **`#[non_exhaustive]` on every public error enum and event enum.** Adding a variant is then not a breaking change.
- **`Envelope` does NOT implement `Clone`.** Structural enforcement: routing graph uses `Arc<Envelope>` ownership; cloning an envelope's bytes is impossible without unwrapping the `Arc`. This is a stronger guarantee than a lint.
- **Public futures returned from libraries are `Send + 'static`:** every `pub fn` returning `impl Future` MUST declare `impl Future<Output = T> + Send + 'static` unless an explicit doc comment marks it as runtime-local. Absent bound silently produces `!Send` futures that fail only at the `tokio::spawn` call site.
- **No borrowed lifetimes in envelope types crossing `await` points.** Envelope types are owned (or `Arc`-wrapped); never `Envelope<'a>` in async signatures.
- **`#[deny(rust_2024_compatibility)]` at every crate root** so we surface edition migration issues immediately if MSRV ever shifts.
- **`dyn Trait` object-safety audit:** crate-root `#![deny(...)]` includes the relevant rustc 1.78+ object-safety lints. Adding an `async fn` to a public trait without `#[async_trait]` or `dyn-compatible(false)` is a build break.

#### MSRV & Compilation

- Build with `RUSTFLAGS="-D warnings"` in CI. Warnings must not accumulate.
- Public API uses only stable features. `cargo +nightly rustfmt` is allowed; `cargo +nightly build` is not.

#### Forbidden in Rust

- ❌ `#[allow(...)]` without a `// SAFETY:`, `// LINT:`, or `// ALLOW:` comment naming the reason and a tracking issue
- ❌ `std::process::exit` from anywhere except the binary's `main` (and only after graceful shutdown drained pending NACKs)
- ❌ `Box<dyn Error>` in public API — use `thiserror`-defined enum errors
- ❌ `RefCell` / `Rc` in async code — use `Arc<Mutex<_>>` or `Arc<RwLock<_>>`
- ❌ `std::mem::forget` and `ManuallyDrop` outside platform-syscall crates
- ❌ Borrowed lifetimes in envelope types crossing `await`

### TypeScript SDK + Dashboard (under Bun)

#### Type System Discipline

- **No `any`.** CI gates `@typescript-eslint/no-explicit-any` at error level; `noImplicitAny: true` in tsconfig handles inferred `any`.
- **No `as` assertions** except `as const`. Type narrowing uses runtime guards (`is`-predicates) backed by TypeBox schemas. Object literals MUST use `satisfies` rather than type assertion to constrain shape without widening inferred types.
- **`@ts-ignore` and `@ts-nocheck` are forbidden unconditionally.** `@ts-expect-error` is permitted **only** with a mandatory trailing comment of the form `// AC-TS-XX: <reason> tracked:<issue-url>`. `@ts-expect-error` self-cleans (build fails when the error disappears) and is the correct suppression for genuinely-unfixable third-party type breakage (e.g., `bun:ffi`, vendored `.d.ts` stubs).
- **`exactOptionalPropertyTypes: true` is load-bearing.** Distinguishes "field absent" (omit from envelope) from "field set to undefined" (explicit clear).
- **`noUncheckedIndexedAccess: true` is load-bearing.** Array/record access returns `T | undefined`.
- **No declaration merging** of `globalThis` or third-party module namespaces without a `// AC-TS-MERGE: <reason>` audit comment.

#### Async / Promise Discipline

- **No floating promises.** ESLint `no-floating-promises` at error.
- **No `Promise.all` on operations that must commit transactionally.** Use `Promise.allSettled` and inspect each result. Enforcement: a custom ESLint rule that flags `Promise.all` calls in files matching `src/transactions/**` or carrying a `// transactional` directive comment, plus a PR-review checklist item for non-conventionally-named call sites.
- **`AbortSignal` propagation is mandatory.** Every async API in the SDK accepts `{ signal }: { signal: AbortSignal }` (non-optional in shutdown-relevant code paths). Cancellation propagates to the underlying socket via `STREAM_END` control frame.
- **No top-level `await` in library code.** Bun supports it, but it changes import-time semantics in ways that break the auto-spawn shim's "library importable without daemon" guarantee.

#### Resource Lifecycle

- **`Symbol.dispose` is required on every type that holds a socket, file handle, or IPC connection.** Resource cleanup uses the `using` keyword, not raw `try/finally`. Raw `try/finally` cleanup of resource-holding types is non-conforming.

#### Module Hygiene

- **No bare side-effect imports** in `src/`. Polyfills live in an explicit `polyfills.ts` entry point that consumers opt into; library code does not poison consumers' tree-shaking via top-level side effects.
- **Default exports are forbidden.** Named exports only.

#### Bun-Specific Rules

- **No `bun --hot` for daemon-adjacent processes.** Hot reload during a stream invalidates `stream_id` correlations and triggers DLQ floods. Allowed for the dashboard frontend only.
- **`Bun.serve` is the only HTTP/WebSocket server in the codebase.** No Hono, Express, Fastify, or alternatives.
- **`bun:sqlite` is forbidden in production code.** SQLite access goes through the daemon. (Permitted in test fixtures only.)
- **`Bun.spawn` is the SDK's auto-spawn primitive.** Do not shell out via `child_process` polyfill.
- **`bun test` is the only test runner.**

#### Schema-Layer Discipline (TypeBox)

- TypeBox types are exported from a single `@zorn/schema` workspace package; SDK and dashboard import from there.
- Use `Type.Strict()` for all envelope schemas (enforces `additionalProperties: false`).
- Use `TypeCompiler.Compile()` for hot-path validation (10× faster than interpreter-based `Value.Check`).

#### Forbidden in TypeScript

- ❌ `enum` in **wire-protocol values** — use union types so the wire shape is structural; `enum` is permitted for internal state machines that never serialize
- ❌ Class-based APIs in the SDK public surface — functional, plain-object returns
- ❌ Generic catch-all middleware (`app.use((err, req, res) => …)`); use Bun.serve's typed error handler
- ❌ Bare side-effect imports outside `polyfills.ts`
- ❌ Default exports

### Python SDK

#### Type Discipline

- **`mypy --strict` passes with zero `# type: ignore`.** Genuine third-party gaps go in a single `mypy.ini` `[mypy-vendor.*]` section with a comment naming the upstream issue. CI uses `mypy --warn-unused-ignores` to surface stale ignores.
- **Public API has runtime type validation via Pydantic v2** (`model_config = ConfigDict(extra='forbid', strict=True)`). `extra='allow'` silently swallows misspelled envelope fields.
- **`from __future__ import annotations`** at the top of every module.
- **No `Any` in public function signatures.**
- **Use `ParamSpec` (not `TypeVar`) for callable-wrapping generics.** Missing this produces `Any`-typed wrappers that pass `mypy --strict` silently under some inference paths.

#### Async Discipline

- **`asyncio` is the primary event loop.** **`anyio` is permitted as a structured-concurrency abstraction layer** (its `TaskGroup` is better designed than bare `asyncio.create_task` and provides cleaner cancellation propagation). `trio` is forbidden directly. `gevent` and `eventlet` are forbidden.
- **Every coroutine that touches I/O or external resources MUST be called under `asyncio.timeout()` (Python 3.11+).** Default timeout values must NOT be `None` — pick a real budget. `timeout=None` is forbidden in library code; `timeout=None` is permitted only in CLI entrypoints where the user is the cancellation source.
- **`asyncio.create_task` results MUST be retained, with mandatory ordering:**

```python
_background: set[asyncio.Task[Any]] = set()

def spawn(coro: Coroutine[Any, Any, T]) -> asyncio.Task[T]:
    t = asyncio.get_running_loop().create_task(coro)
    _background.add(t)          # add BEFORE callback registration
    t.add_done_callback(_background.discard)
    return t
```

  The add-before-register ordering closes a documented race where a fast-completing task fires `discard` before `add` has registered the reference. CI gate: custom `ruff` rule `ZORN001`.

- **No `asyncio.run()` in library code.** Library code runs inside the caller's loop. Permitted only in CLI entrypoints and tests.
- **`async with` for connection lifetime.** `connect()` returns an async context manager.
- **Public async generators MUST be consumed via `async for` with `contextlib.aclosing()`.** Bare `__anext__()` calls are forbidden — they leave `finally` blocks unrun on early exit.
- **`ContextVar` non-propagation across `create_task` boundaries is documented behavior.** SDK code that mutates a `ContextVar` inside a child task does NOT propagate the change back to the parent; any code relying on shared `ContextVar` mutation across task boundaries is a bug.

#### Validation Discipline

- Pydantic v2 `@model_validator(mode='after')` is the **construction-time gate** for cross-field invariants (e.g., reject `at_most_once` on `request` envelopes). "Construction time" is the correct phrasing — Python has no compile or import phase for user-class instantiation invariants.
- JSON serialization uses `model_dump(mode='json', by_alias=True)`. Mixed mode emits Python `datetime` objects the schema gate rejects.
- A meta-test imports every public model class and asserts `model_config['extra'] == 'forbid'` and `model_config['strict'] == True`. Pydantic config is not lint-checkable.

#### Concurrency / GIL Awareness

- No threads in the SDK. `asyncio` (or anyio) only.
- No `multiprocessing` in the SDK.

#### Test Discipline

- Test patching uses `pytest`'s `monkeypatch` fixture or `unittest.mock.patch` as a context manager only. Direct attribute assignment on imported modules contaminates the test process.

#### Forbidden in Python

- ❌ `print()` for diagnostics — use the `logging` module (the daemon ships an `OTLPLogHandler`)
- ❌ Bare `except:` and `except Exception:` without re-raise or named handler
- ❌ `time.sleep()` in async code — `await asyncio.sleep()`
- ❌ `requests`, `httpx` (sync mode), `urllib3` direct — the SDK has its own UDS transport
- ❌ Mutable default arguments
- ❌ Bare `__anext__()` calls; `aclosing()` is mandatory
- ❌ `asyncio.create_task()` results discarded
- ❌ `timeout=None` in library code
- ❌ `trio`, `gevent`, `eventlet`, threads in SDK, multiprocessing in SDK

### Cross-SDK Invariants

#### Naming

- Public types: `PascalCase`
- Functions / methods: `snake_case` in Rust + Python; `camelCase` in TypeScript
- Envelope field names on the wire: **always `snake_case`**, regardless of emitting SDK
- SDKs translate idiomatic case ↔ wire `snake_case` at serialization, not at API surface

#### Versioning — split lockstep / per-SDK

A repo-root `version.toml` defines two version axes:

- **`sdk_version`** (semver) — the wire-protocol/envelope version. **Lockstep across all three SDKs.** A change here requires all three SDKs to publish the same version atomically.
- **`build_version`** (per-SDK semver) — the SDK build/security-patch version, drifts independently. A Rust security patch bumps `zorn-mesh-rust.build_version` without touching the TS or Python builds.

CI gate (`xtask check-versions`): `sdk_version` field in `Cargo.toml`, `package.json`, and `pyproject.toml` MUST equal `version.toml:sdk_version`. `build_version` is independent.

#### Logging

- All three SDKs route logs through the OpenTelemetry logging API after bootstrap completes — never directly to stdout/stderr in library code.
- Log levels: `trace` / `debug` / `info` / `warn` / `error`. No custom levels.
- Structured fields, not interpolated strings. The OTel handler captures structured fields as span attributes; interpolation loses them.

#### Bootstrap Logging Contract (`ZORN_BOOTSTRAP`)

The auto-spawn moment is *before* the host's OTel SDK is initialized, so library code cannot route bootstrap-phase logs through OTel. The contract:

- **Bootstrap-phase logs go to stderr in structured JSON**, prefixed with `ZORN_BOOTSTRAP:`:
  ```
  ZORN_BOOTSTRAP: {"level":"INFO","source":"zorn-meshd","msg":"socket bound","path":"/run/user/1000/zorn-mesh/zorn.sock","ts":"2026-04-26T15:59:55.294000Z"}
  ```
- **The library reads, parses, and re-emits these as OTel log records** once the host's OTel SDK is initialized, preserving timestamps and severity.
- All other diagnostic output (post-bootstrap) goes through OTel directly.

This protocol applies to the daemon binary, the SDK auto-spawn shim in each language, and the CLI.

#### Observability Conventions

- Span names follow `zorn.<component>.<operation>` (e.g., `zorn.sdk.connect`, `zorn.daemon.route`, `zorn.codec.validate`).
- Span attributes use OTel semantic conventions where they exist; Zorn-specific attributes prefix `zorn.*`.
- Errors recorded with `span.record_exception(...)` and `span.set_status(Status(StatusCode.ERROR, ...))` — never just logged.

### Operational Discipline (cross-language, long-lived-daemon focus)

These rules address failure modes that bite long-lived local daemons hardest. They apply to the daemon, all three SDKs, and the CLI.

#### Filesystem Watchers

- All filesystem watches go through a single watcher abstraction per process. No ad-hoc `inotify_init1` / `kqueue` / `FSEvents` calls scattered across modules.
- Watch paths are validated at startup; non-existent paths are explicit errors, not silent no-ops.
- Event handlers MUST be idempotent — coalesced events under write bursts will deliver fewer notifications than there were writes.
- Watch lifetime is bounded by the owning component; orphaned watches are a leak.
- Recursive watches on large trees are forbidden without an explicit fd-budget check at startup.

#### Signal Handling

- **SIGPIPE is explicitly ignored at process startup** (`signal(SIGPIPE, SIG_IGN)` in Rust; `process.on('SIGPIPE', () => {})` in Bun; `signal.signal(signal.SIGPIPE, signal.SIG_IGN)` in Python). Default behavior kills the process when a client disconnects mid-write.
- **SIGTERM handler completes within 5 seconds**, signals the main loop via a channel, and never calls `exit()` directly. Graceful shutdown drains pending NACKs before exit.
- **SIGHUP triggers config reload, not restart.** Daemons that want restart semantics use SIGUSR1.
- **SIGINT** in interactive contexts (CLI) initiates the same graceful shutdown as SIGTERM.

#### Graceful Shutdown Timing

- Total shutdown budget: **10 seconds** from signal receipt to process exit. Configurable via `ZORN_SHUTDOWN_BUDGET_MS` (max 60 s).
- Ordering, strictly:
  1. Stop accepting new work (close UDS listener / WebSocket gateway).
  2. Drain in-flight envelopes — emit NACK with `shutting_down` reason for any envelope that cannot complete in budget.
  3. Flush OTel batch span/log processors.
  4. Release resources (close DB, fsync WAL, unlink rendezvous files if owned).
- Every exit path logs shutdown reason + duration via the bootstrap logging contract (since OTel may already be flushed).

#### Environment Variable Discipline

- Env vars are read at startup only, into a typed config struct. No mid-request `os.getenv` / `std::env::var` / `process.env`.
- Secrets-bearing env vars are NEVER logged or included in span attributes. The config struct's `Debug`/`Display`/`__repr__`/`toJSON` implementation MUST redact secret fields.
- The full list of recognized env vars is documented at `/docs/env-vars.md`. Unknown `ZORN_*` env vars at startup emit a warning span.

#### Secret Material in Process Memory

- Secret values (API keys, signing keys, capability tokens) are wrapped in language-appropriate types that suppress accidental disclosure:
  - **Rust:** the `secrecy` crate (`Secret<String>`); the inner value is accessed only through `expose_secret()`.
  - **TypeScript:** a `Secret<T>` class that overrides `toJSON`, `toString`, and `Symbol.toPrimitive` to return `'<redacted>'`; the inner value is accessed only through `.unwrap()`.
  - **Python:** a `Secret` dataclass whose `__repr__` and `__str__` return `'<redacted>'`; the inner value is accessed only through `.expose()`.
- Secret values MUST NOT appear in span attributes, log fields, or error messages — even via `Debug`-style formatting.
- Where the language permits (Rust via `Zeroize` derive), secret memory is zeroed on drop.

### Test Harness Protocol

The shared daemon fixture from Cat 1 is implemented as a **single `zorn-test-harness` binary** that all three SDK test suites invoke. The harness speaks JSON over stdin/stdout:

**Operations (stdin):**
```json
{"op": "start", "agents": [{"id": "agent-a", "card": {...}}]}
{"op": "register", "agent_id": "agent-c", "card": {...}}
{"op": "send", "envelope": {...}}
{"op": "assert_delivered", "message_id": "...", "timeout_ms": 1000}
{"op": "inject_fault", "kind": "delay", "duration_ms": 500}
{"op": "stop"}
```

**Events (stdout):**
```json
{"event": "ready", "socket": "/tmp/zorn-test-XXXX.sock"}
{"event": "delivered", "message_id": "...", "to": "agent-b"}
{"event": "nack", "message_id": "...", "code": 4, "reason": "..."}
{"event": "shutdown", "reason": "client_request"}
```

Each SDK has a thin (~50-line) adapter wrapping this harness:

- **Rust:** `ZornHarness::start().await?` returning a typed handle.
- **TypeScript (Bun):** `await ZornHarness.start()` returning a typed object.
- **Python:** `async with zorn_harness() as h:` (async context manager).

Identical semantics, three thin wrappers over one shared binary. Conformance tests reuse the same operation sequences across all three SDKs.

### Forbidden-Pattern Enforcement Matrix

Every rule above is classified as **machine-enforced** (CI fails the build) or **human-enforced** (PR review or meta-test). Ambiguity is gone.

| Rule | Class | Mechanism |
|---|---|---|
| no `# type: ignore` | machine | `mypy --warn-unused-ignores` + grep CI step |
| no `@ts-ignore`/`@ts-nocheck` | machine | `@typescript-eslint/ban-ts-comment` (error) |
| no `any` (explicit) | machine | `@typescript-eslint/no-explicit-any` (error) |
| no `any` (inferred) | machine | `noImplicitAny: true` |
| no `unwrap()`/`expect()` outside tests | machine | `#![deny(clippy::unwrap_used, clippy::expect_used)]` + `#[cfg(test)] #![allow(...)]` |
| no `block_on` | machine | `clippy::disallowed_methods` |
| no `tokio::spawn` in routing | machine | per-crate `clippy::disallowed_methods` config |
| no floating promises | machine | `@typescript-eslint/no-floating-promises` (error) |
| no `Promise.all` on transactional ops | mixed | path-convention ESLint rule (`src/transactions/**`) + PR-review checklist for non-conventional sites |
| `Envelope` does not implement `Clone` | machine (structural) | the type does not derive `Clone`; calling `.clone()` won't compile |
| no `asyncio.run` in libs | machine | `ruff` rule `ASYNC116` |
| no threads/multiprocessing in SDK | machine (partial) | `ruff` flags static imports; dynamic imports caught in PR review |
| Pydantic `extra='forbid'` on every model | human | meta-test importing all public models, asserting config |
| `create_task` add-before-register ordering | machine | custom `ruff` rule `ZORN001` |
| `Symbol.dispose` on resource types | machine | custom ESLint rule asserting class members on types tagged `@resource` JSDoc |
| no bare side-effect imports | machine | ESLint `import/no-unassigned-import` configured to allow only `polyfills.ts` |
| `satisfies` over type assertions | machine | `@typescript-eslint/consistent-type-assertions` configured |
| MSRV check | machine | `cargo-msrv verify` in CI |
| `version.toml` consistency | machine | `xtask check-versions` |
| Bootstrap log format | human | structured-log assertion in integration tests |
| OTel span name conventions | human | dashboard-driven spot check; review |
| Signal handler discipline | human | code review + integration test invoking signals |
| Shutdown budget compliance | machine | integration test with timing assertion |

Rules without a named mechanism are decoration and must not be added to the spec.

### Language-Specific Chaos Packs

Cross-SDK protocol-level conformance catches roughly 60% of real failure modes. The remaining 40% — disproportionately the silent-data-loss and process-hang failures — only show up under language-idiomatic chaos. Each SDK ships its own pack:

```
tests/
  conformance/        # cross-SDK protocol level (Cat 2 anchors)
  chaos/
    cross_sdk/        # wire-level fault injection
    rust/             # tokio panic propagation, JoinSet drain, select! cancel-safety, error-chain downcast
    python/           # CancelledError propagation, create_task drop, ContextVar leak, async-generator cleanup
    typescript/       # unhandledRejection in Bun, Bun.spawn target death, microtask starvation, AbortSignal propagation
```

**Python cancellation suite ships first.** Coroutine cancellation at `await` points is the most commonly mishandled asyncio invariant and is completely invisible at the protocol level — it is the highest 2am-page risk.

---

## Persistence & Reliability Rules

These rules govern how Zorn Mesh writes, reads, retains, and recovers state. SQLite is the only persistence engine (Cat 1); these rules say *how* it is used.

### Schema Layout

The daemon owns one SQLite database file at `${ZORN_DATA_DIR}/zorn-mesh.db` (default: `${XDG_DATA_HOME}/zorn-mesh/`, falling back to `~/.local/share/zorn-mesh/` on Linux, `~/Library/Application Support/zorn-mesh/` on macOS).

Tables (mandatory):

- **`agents`** — registered agents, keyed by `AgentCard.id`.
- **`capabilities`** — agent capabilities, unique on `(agent_id, name, version)`.
- **`messages`** — append-only message log for `at_least_once` envelopes. Indexes: `(to_agent_id, status, ingress_ts)`, `(trace_id)`, `(correlation_id)`.
- **`dead_letters`** — exhausted-retry messages, FK to `messages.message_id`.
- **`idempotency_keys`** — dedup table; PK `(key, to_agent_id, capability_ref)`; index on `(expires_at)`.
- **`audit_log`** — registration, capability changes, security events.
- **`leases`** — capability ownership leases.
- **`traffic_samples`** — `(sampled_at INTEGER PRIMARY KEY, envelope_count INTEGER)`. Written by the writer task every 30 s; consumed by operator tooling for reporting (NOT by automated VACUUM scheduling — see VACUUM Discipline below).
- **`schema_migrations`** — `sqlx migrate` bookkeeping. Owned by the migration tool; do not write directly.

All `INTEGER` timestamp columns store **microseconds since Unix epoch**, UTC. No SQLite `DATETIME`/`TEXT` timestamps.

### Connection Pool — Writer / Reader Separation

The daemon opens connections via two distinct sqlx pools, each with class-specific pragmas. *(This refines Cat 1's blanket `synchronous=NORMAL`: readers preserve the performance default; the single writer is upgraded so the "ACK only after COMMIT" invariant is actually durable.)*

**Writer pool:** `max_connections = 1` (SQLite is single-writer; coordinating multiple writer connections in-process adds zero throughput and complicates lock semantics).

- `journal_mode = WAL`
- **`synchronous = FULL`** *(refines Cat 1)* — fsync per commit. ~1 ms overhead on NVMe, eliminates the "OS-crash loses last checkpoint window" hole. ACK-after-COMMIT becomes a real durability guarantee.
- `temp_store = MEMORY`
- `mmap_size = 67108864` (64 MiB) *(reduced from 256 MiB)*
- `cache_size = -16000` (16 MiB) *(reduced from 64 MiB)*
- `busy_timeout = 5000`
- `foreign_keys = ON`

**Reader pool:** `max_connections = 5` (`ZORN_DB_READ_POOL_SIZE`, max 16). All readers opened `SQLITE_OPEN_READONLY`.

- `journal_mode = WAL` (read-only, doesn't write WAL)
- **`synchronous = NORMAL`** — readers don't write; the pragma controls reader fsync of WAL recovery only.
- `temp_store = MEMORY`
- `mmap_size = 67108864` (64 MiB shared mapping; not multiplied by reader count in practice)
- `cache_size = -16000` (16 MiB per reader → 80 MiB total)
- `query_only = ON`

**Total memory budget:** ~96 MiB SQLite cache + ~64 MiB mapped DB region. Order of magnitude smaller than the original draft's ~1 GiB ceiling, fits comfortably alongside Cursor + Claude Desktop + browser on a 16 GiB host.

**No connection-string-based PRAGMA settings** — they don't survive `ATTACH`. Always set programmatically post-acquire.

### Write Discipline

- **Every envelope-persisting write is wrapped in a transaction**, even single-row inserts.
- **Writes use `sqlx`'s compile-time-checked `query!` and `query_as!` macros** (Cat 1). Dynamic SQL string concatenation is forbidden in routing code; the migration tool is the only place dynamic SQL is permitted.
- **Idempotency check precedes `pool.begin()`.** The first read query in every write path looks up the dedup key; on hit, the writer returns the cached `message_id` immediately without acquiring a writer transaction. Only a miss proceeds to `BEGIN`.
- **The 50 ms transaction budget is anchored to envelope arrival at the writer's mpsc inbox**, not to `BEGIN`. This makes inbox queue depth visible in latency percentiles. Implementation:

```rust
// AC-P-01: budget timer starts at inbox dequeue
let inbox_arrival = envelope.recv_ts;
let mut tx = pool.begin().await?;
// ... queries ...
tx.commit().await?;
let elapsed = inbox_arrival.elapsed();
if elapsed > Duration::from_millis(50) {
    tracing::warn!(target:"zorn_mesh::writer::slow_tx",
                   elapsed_ms = elapsed.as_millis(), "db_slow");
    // emit NACK with db_slow reason
}
```

- **Payloads >1 MiB rejected at ingress** with a registered error code; this is enforced before the envelope reaches the writer's inbox.
- **The message-log INSERT is the commit point for `at_least_once` delivery.** Sequence is strictly: ingest → validate → idempotency lookup → BEGIN → INSERT message → INSERT idempotency_key → COMMIT → emit ACK control frame.

### WAL Checkpoint Discipline

- `wal_autocheckpoint = 1000` pages (default).
- `WalKeeper` actor runs `PRAGMA wal_checkpoint(PASSIVE)` every 60 s.
- WAL >64 MiB → WalKeeper forces `PRAGMA wal_checkpoint(TRUNCATE)` and emits a warning span. (TRUNCATE replaces the original draft's hourly low-traffic detection — it runs only when the WAL has actually grown large, not on a fragile schedule.)
- Sustained large WAL → daemon emits `db_wal_unbounded` health event for the dashboard.
- **No `PRAGMA wal_checkpoint(RESTART)` from routing code.**

### VACUUM Discipline

- **`auto_vacuum = INCREMENTAL` is set at schema creation** (migration `0001_init.sql`). This is a one-time-at-DB-init setting; changing it later requires a full VACUUM, which is operator-scheduled (see below).
- **`PRAGMA incremental_vacuum(64)` runs from inside RetentionWorker pause windows.** The 100 ms pause between retention batches is enough time for ~64 free pages to be reclaimed without blocking readers. This replaces the original draft's weekly-VACUUM-on-schedule entirely.
- **Operator-scheduled full VACUUM is a manual escape hatch:** `zorn maintenance vacuum` (CLI subcommand) blocks writers, runs `VACUUM`, and reports duration. Used only when significant manual reclamation is needed (e.g., after a large retention purge). Not automated. Not on a cron schedule. Not invoked from the daemon's hot path.
- Traffic-window forecasting is removed entirely. The `traffic_samples` table is retained for operator reporting only.

### Idempotency Table Maintenance

- The `idempotency_keys` table is pruned every 30 s by a dedicated `IdempotencyPruner` actor.
- **Pruning uses an explicit transaction:**

```sql
BEGIN DEFERRED;
DELETE FROM idempotency_keys
  WHERE expires_at < :cutoff
    AND message_id NOT IN (
      SELECT message_id FROM messages
       WHERE status IN ('pending','delivering')
    );
COMMIT;
```

  Explicit `BEGIN DEFERRED` is mandatory, not optional — even though SQLite's single-statement atomicity makes the subquery snapshot-consistent, the explicit transaction makes the intent auditable and ensures any future query change preserves the safety property.

- **The dedup INSERT is part of the same transaction as the message INSERT.** PK collision aborts the transaction; the daemon returns the `message_id` from the existing key row.
- The pruner uses the writer pool (it writes); it MUST NOT use a reader connection.

### Retention & Compaction

- **Default retention:** 24 h for `messages`, 7 days for `dead_letters`, 30 days for `audit_log`. Configurable per-table via daemon flags.
- **Retention enforcement:** `RetentionWorker` actor runs every 5 min, deleting rows older than the per-table boundary in batches of 1000 with a 100 ms pause between batches. `incremental_vacuum(64)` runs during each pause.
- **Replay safety:** retention deletes only `status IN ('delivered', 'failed')` rows. `pending` or `delivering` rows older than retention emit a `stuck_message` warning span and are preserved indefinitely until manual triage via `zorn dlq inspect`.
- **Audit log is never truncated for compliance reasons; only dropped via retention boundary.**

### Replay Semantics

- **Replay is read-only.** Re-emits matching messages without altering `status`, `attempts`, or any other column.
- **Replayed envelopes are tagged.** A `replay_session_id` (UUIDv7) and `replay_origin: "zorn-replay"` field are added to the `meta` partition of the canonical envelope.
- **Replay never bypasses idempotency.** If an agent has already acked a message and the dedup window is still open, the daemon NACKs the replay with `idempotency_collision`.
- **Replay over a closed dedup window re-delivers.** Operators are responsible for ensuring the target agent can handle re-delivery.
- **Replay coordinates with retention.** A `replay_session_id` registers a soft hold in memory: the RetentionWorker checks active replay sessions before deleting rows and skips any row a live replay session is iterating over. If a replay session is killed mid-iteration (operator cancels, daemon restart), the soft hold expires after 5 minutes.

### Migrations

- All migrations live in `/migrations/NNNN_<description>.sql`, monotonically numbered.
- **Forward-only.** No `down.sql` — migrations are the source of truth, not reversible operations.
- **Each migration is wrapped in a transaction** (`sqlx migrate` does this automatically). A failed migration leaves the database at the prior version; the daemon refuses to start until resolved.
- **Migration tests** apply each migration to a blank DB, seed known state at each version, and verify daemon startup. Required CI gate.
- **No data backfills in migrations.** Schema-only changes; data backfills run as separate one-shot tasks invoked via `xtask backfill <name>`.
- **Migration concurrency:** the daemon refuses to start migrations while an active connection holds a WAL reader lock. Operator-side `zorn migrate` commands first stop the daemon (or wait for graceful shutdown), then run migration, then restart the daemon. Live-migration during traffic is out of scope.

### Reliability Invariants

- **No envelope is acknowledged to the sender before its `messages` row is committed AND fsynced.** Writer pool's `synchronous=FULL` makes COMMIT block on `fdatasync`; only after `tx.commit().await?` returns does the writer emit the ACK control frame on the UDS socket.
- **DLQ move is a single transaction:** `BEGIN; INSERT INTO dead_letters ...; UPDATE messages SET status='failed' WHERE message_id=?; COMMIT;`. Crash mid-transaction leaves the row at its prior status; the retry loop picks it up on next pass. (Original draft used `DELETE FROM messages` — replaced with status update so the original envelope remains queryable for forensics.)
- **Retry budget is per-message, not per-attempt.** Exceeding `max_retries` (per capability) OR `ttl_ms` (Cat 2) moves the message to `dead_letters`.
- **Retry backoff is exponential with jitter, bounded:** delay = `min(base * 2^attempt, ceiling) ± jitter` where `base = 100 ms`, `ceiling = 5 s`, `jitter = ±25%`. This prevents the busy_timeout cascade — under writer contention, retried envelopes don't pile back into the inbox at full throttle.
- **DLQ is observable.** Every move emits a span with the originating envelope's `trace_id`, the failure reason code, and the attempt count.
- **DLQ entries can be replayed manually** via `zorn dlq replay <message_id>`. Re-delivery resets `attempts` to 1.
- **Crash recovery on startup:** the daemon scans `messages WHERE status = 'delivering'` and resets each row to `status = 'pending'`. Crash mid-delivery is indistinguishable from "consumer slow"; consumer-side idempotency dedup is the safety net. *Conformance test (Cat 4 anchor) MUST assert on the consumer's idempotency store state, not just the daemon's status column.*

### Backpressure & Resource Bounds

- **Per-agent outbound queue:** bounded at 1024 envelopes by default (`ZORN_AGENT_QUEUE_SIZE`). Exceeding the cap triggers a flow-control NACK.
- **Daemon writer mpsc inbox:** 4096 envelopes. Inbox full → flow-control NACK.
- **`busy_timeout` exceeded → NACK with `db_busy` reason.** Daemon emits a `db_contention` span. Sender's retry loop applies the bounded exponential backoff (above) so retries don't cascade.
- **Disk full:** detected via `ENOSPC` on write. Daemon transitions to `degraded` mode: refuses new envelopes with `disk_full` NACK, continues servicing reads, emits a critical health event. **Startup-after-disk-full edge case:** if the daemon restarts under ENOSPC and the `delivering→pending` recovery scan itself fails, the daemon refuses to enter the running state, emits a `recovery_blocked` health event, and exits with a non-zero status. The operator must free disk space before restart.
- **`mmap_size`** is bounded; the daemon does not grow the mapping under load.

### Backup & Restore Protocol

- **`zorn backup <path>` produces a consistent snapshot** with full WAL durability. Sequence:
  1. `PRAGMA wal_checkpoint(FULL)` — flushes all committed WAL frames into the main DB file.
  2. Capture `(SELECT max(message_id), strftime('%s', 'now'))` as the **last consistent timestamp**; record this in the backup metadata sidecar (`<path>.meta.json`).
  3. `VACUUM INTO '<path>'` — produces a defragmented snapshot.
  4. Verify the backup with `PRAGMA integrity_check` opened against the new file.
- **No replication.** If an operator wants offsite copies, they ship the file produced by `zorn backup`.
- **`zorn restore <path>` is manual and stops the daemon first.** Sequence:
  1. Daemon must be stopped (CLI verifies via PID file / lock).
  2. The current DB is moved to `zorn-mesh.db.bak.<timestamp>`.
  3. The backup file is copied into place.
  4. **`PRAGMA journal_mode = WAL` is re-applied** — `VACUUM INTO` produces a non-WAL DB; without this step the daemon would silently run in rollback-journal mode after restore. (This is the single most likely silent corruption path — explicit step in the spec.)
  5. `PRAGMA integrity_check` runs and MUST return `ok`.
  6. `PRAGMA foreign_key_check` runs and MUST return empty.
  7. Daemon starts; startup scan completes.
  8. Smoke test: a synthetic publish round-trips end-to-end.
  9. The restore tool prints the **last consistent timestamp** from the backup metadata, naming the data-loss window between backup time and current time. Operator MUST acknowledge the gap.
- **Operator AC for "system is recovered":** all of the integrity, foreign-key, startup-scan, and smoke-test checks above pass, AND the operator has acknowledged the last-consistent-timestamp gap.

### Forbidden Patterns

- ❌ **Acking before the `messages` row is committed AND fsynced.** Writer pool's `synchronous=FULL` makes this structurally enforced.
- ❌ **Dynamic SQL string concatenation outside `/migrations/` and `xtask backfill`.**
- ❌ **`SELECT *`.** Always enumerate columns.
- ❌ **Long-running queries on the writer connection.** Reads use the read-only pool.
- ❌ **`PRAGMA wal_checkpoint(RESTART)` from routing code.**
- ❌ **`down.sql` migrations.**
- ❌ **Data backfills inside migrations.**
- ❌ **Retention deleting `pending`/`delivering` rows.**
- ❌ **Direct table access from the SDK.** SDKs never open the database file; all access goes through the daemon's API.
- ❌ **Storing payloads >1 MiB.**
- ❌ **`PRAGMA writable_schema = ON`.**
- ❌ **Restoring a backup without re-applying `journal_mode=WAL`.** Silent rollback-journal-mode regression.
- ❌ **Running automated full `VACUUM`.** Use `auto_vacuum=INCREMENTAL` + `incremental_vacuum(N)` from RetentionWorker pauses; full VACUUM is operator-invoked only.

### Conformance Test Anchors

Every "MUST"/"MUST NOT" rule above is anchored to a fixture under `/conformance/persistence/`:

- `persistence/ack-after-fsync/` — fault hook fires SIGKILL post-WAL-write but pre-`fdatasync`-return; verify the message is **redelivered** on restart AND that no ACK was observed by the consumer (transport-layer ACK probe required, not just status column check).
- `persistence/wal-checkpoint/` — sustained write triggers auto-checkpoint at threshold; WAL >64 MiB triggers TRUNCATE.
- `persistence/incremental-vacuum/` — RetentionWorker pause windows execute `incremental_vacuum(64)`; cumulative free-page count decreases; readers are never blocked >5 ms.
- `persistence/idempotency-prune/` — concurrent prune + in-flight delivery; prune never deletes a key whose message is `pending` or `delivering`. Explicit unit test: insert key with `expires_at = now-1s`, mark message `pending`, run pruner, assert key NOT deleted.
- `persistence/dlq-atomicity/` — transaction-step-aware fault hook fires between `INSERT INTO dead_letters` and `UPDATE messages SET status='failed'` within the same transaction; verify message remains queryable in `messages` (status unchanged) AND no orphan row in `dead_letters` after restart.
- `persistence/retention/` — rows older than retention deleted; `pending` rows preserved; `audit_log` retention independent.
- `persistence/replay-during-prune/` — start replay of 1000 messages, trigger retention prune mid-replay; assert all 1000 delivered (replay session's soft hold prevented prune) OR a clear error with `dropped_count` in span attributes. Silent partial delivery is a failure.
- `persistence/migration-recovery/` — partial migration aborts cleanly; daemon refuses to start; resolved migration succeeds on retry.
- `persistence/migration-mid-checkpoint/` — migration attempted while WAL checkpoint is in progress; assert deterministic ordering (migration waits OR cleanly fails with named reason); no DB corruption.
- `persistence/crash-recovery/` — `delivering` rows reset to `pending` on startup; retry loop resumes; **consumer-side idempotency store is asserted** to have the message recorded exactly once after redelivery (not just daemon status check).
- `persistence/busy-timeout-cascade/` (P0) — hold writer lock via debug socket for 10 s, fire 100 concurrent senders; assert inbox depth does NOT monotonically increase; at least 50% of messages complete after lock release within budget.
- `persistence/disk-full-then-crash/` (P0) — fill disk to 95%, inject crash, restore power; verify startup-scan either completes OR fails with `recovery_blocked` health event and non-zero exit. No hang.
- `persistence/concurrent-idempotency-collision/` (P1) — 10 concurrent sends with identical idempotency keys; assert exactly 1 message in `messages` table after all complete; remaining 9 each receive the cached `message_id` of the first.
- `persistence/backup-checkpoint/` — 100 msg/s synthetic load during `zorn backup`; row count in backup ≥ row count in live DB at backup-start-time (no rows missed); backup metadata sidecar contains valid `last_consistent_timestamp`.
- `persistence/restore-wal-mode/` — `zorn restore` produces a DB in `journal_mode=WAL` after the procedure completes (regression test for the silent rollback-mode bug).
- `persistence/restore-data-loss-window/` — backup at T₀, generate messages M₁..Mₙ between T₀ and T₁, restore at T₁; assert M₁..Mₙ are NOT in the restored DB AND the operator-facing output names the gap with the timestamp.

---

## Security Model

The threat model and zero-trust authorization rules. Authentication primitives (UDS `SO_PEERCRED`, ed25519 signatures, AgentCard identity) come from Cat 1–2; this category consolidates them into a coherent policy layer.

### Threat Model

Zorn Mesh runs on a single user's machine. The threats it explicitly defends against:

1. **Untrusted local processes** — malicious code, compromised developer tooling, or third-party agents the user installed without full trust. Default-deny: a new agent can register but cannot invoke any privileged capability without explicit allowlisting.
2. **Prompt injection cascading through agents** — a low-privileged research agent receives poisoned context, decides to invoke a high-privileged execution agent. The daemon labels every message with originator `trust_level` so consuming agents can apply mitigations; high-privilege capabilities require explicit `trust_level` policy.
3. **Compromised agent processes** — an agent process is exploited (memory corruption, supply-chain compromise). Mitigation: per-agent capability tokens with short TTL, revocable mid-session; signed envelopes for high-sensitivity capabilities; daemon-side audit of every privileged invocation.
4. **Credential theft from process memory** — secrets in long-lived agent memory are exposed via crash dumps, `/proc/[pid]/mem`, or core files. Mitigation: secret redaction wrappers (Cat 3), `Zeroize` on drop in Rust; capability tokens are short-lived rather than long-lived.
5. **Tampered messages on the loopback HTTP gateway** — browser-originated traffic via the dashboard is treated as untrusted; CSRF-token-bound session, gateway-injected `gateway_verified` flag, identity claims informational only (Cat 2).

The threats explicitly **out of scope**:

- Multi-user host isolation (one user, one daemon, one socket).
- Network-attacker threats (no network listener; loopback HTTP gateway is bound to 127.0.0.1 only).
- Physical access to the host (out of scope; user owns the machine).
- Side-channel attacks (Spectre, Rowhammer, etc.) on shared hardware (out of scope; mitigations live at OS/CPU layer).
- Supply-chain attacks on the Rust/Bun/Python toolchains (mitigated by lockfile discipline + `cargo-audit` / `bun audit` / `uv pip audit` CI gates, but the daemon does not attempt runtime detection).

### Authentication

Authentication is layered. Each layer adds a higher-confidence binding to identity.

#### Layer 1 — OS Process Identity (always)

- The daemon calls `SO_PEERCRED` (Linux) / `LOCAL_PEERCRED` + `LOCAL_PEEREPID` (macOS) on every UDS connection accept.
- Returns: `(uid, gid, pid)`.
- This identity is **non-spoofable** from userspace; only the kernel can vouch for it.
- Filled into `zorn_envelope.identity` for every envelope crossing the UDS path.

#### Layer 2 — AgentCard Registration (always)

- After connect, the agent calls `agent.register` with its A2A AgentCard (Cat 2 envelope).
- The daemon binds `AgentCard.id` to the connection's `(uid, gid, pid)` triple.
- Subsequent envelopes from this connection MUST carry the same `AgentCard.id`; mismatch → `NACK identity_mismatch` and connection close.
- AgentCard.id stability across restarts is mandatory (Cat 2). The daemon's `agents` table records first-seen `(uid, gid)` per `AgentCard.id`; a registration from a different `(uid, gid)` for an existing `AgentCard.id` triggers a `trust_level_changed` audit event and downgrades the agent's `trust_level` until manually reapproved.

#### Layer 3 — Capability Token (for privileged capabilities)

- High-privilege capabilities (those tagged `requires_token = true` in their AgentCard descriptor) require the agent to present a **capability token** issued by the daemon.
- Tokens are JWT-formatted, signed by the daemon's session key (rotated daily), and carry: `agent_id`, `capability_ref`, `issued_at`, `expires_at` (TTL ≤ 5 minutes), `nonce`.
- Token issuance is logged in `audit_log` with the requesting agent's full identity context.
- Tokens are **short-lived by design**: an agent re-requests a token before each privileged invocation (or batches for a 5-minute window). Long-lived tokens are forbidden — credential theft has bounded blast radius.

#### Layer 4 — Signed Envelopes (for high-sensitivity capabilities)

- Capabilities tagged `requires_signature = true` mandate ed25519 signature verification on the envelope.
- Each agent registers a public key as part of its AgentCard. The daemon verifies the signature on inbound envelopes for these capabilities.
- Private keys are agent-managed; the daemon never sees them. Key rotation is operator-driven via `zorn agent rotate-key`.
- Signed envelopes carry `security.signature` (base64), `security.key_id`, `security.alg = "ed25519"`, `security.nonce`, and `security.anti_replay_window_ms`.

#### Layer 5 — WebSocket Gateway Path (informational only)

- For `transport = "ws_gateway"` envelopes (Cat 2), `(uid, gid, pid)` are **informational only**.
- The gateway issues a one-time session token at WebSocket handshake; downstream authz uses the session token.
- The gateway tags envelopes with `gateway_verified: true|false`. A `false` flag means CSRF check failed; daemon refuses such envelopes.
- Browser-originated requests cannot acquire `requires_signature = true` capabilities — there is no key-binding path. Operator must use the CLI or a UDS-connected agent for those.

### Trust Levels

Every agent is assigned a `trust_level` at registration time:

- **`system`** — daemon-internal agents (the CLI process, the dashboard gateway, the WalKeeper / RetentionWorker / IdempotencyPruner internal actors). Can invoke any capability. Cannot be assigned to externally-registered agents.
- **`trusted`** — explicitly approved by the operator via `zorn allow <agent_id>` or via a config-file allowlist. Default for agents whose `(uid, gid)` matches the daemon's running user.
- **`untrusted`** (default for new registrations) — the safe default. Can invoke capabilities tagged `trust_level_required = "untrusted"` or higher (i.e., capabilities that explicitly accept untrusted callers). Cannot invoke privileged capabilities without explicit per-capability allowlisting.

Trust-level transitions are operator-driven only:

- `zorn trust <agent_id> <level>` upgrades or downgrades.
- Every transition emits an `audit_log` entry with operator identity, prior level, new level, and reason text.
- Downgrades take effect immediately; in-flight envelopes from the agent at the old level complete normally; new envelopes use the new level.

### Authorization Policy

The daemon owns a **default-deny** authorization decision point. Every envelope that addresses a capability passes through this decision before delivery.

#### Decision Inputs

For each envelope routed to a capability:

1. The caller's `AgentCard.id` and `trust_level`.
2. The caller's `(uid, gid, pid)` triple (from `SO_PEERCRED`).
3. The capability's `trust_level_required` (declared in the target agent's AgentCard).
4. The capability's `requires_token` and `requires_signature` flags.
5. The presence and validity of any capability token in `zorn_envelope.security.token_id`.
6. The signature verification result, if `requires_signature = true`.
7. The capability's per-agent allowlist (if defined; opt-in mechanism).

#### Decision Logic

```
permit if (
    capability.trust_level_required is met by caller.trust_level
    AND (not capability.requires_token OR token is valid and not expired)
    AND (not capability.requires_signature OR signature is verified)
    AND (capability.allowlist is empty OR caller.agent_id is in allowlist)
)
else deny
```

Deny → `NACK authz_denied` with a registered error code naming the failing condition (without leaking authorization-internal details). Audit log records the full decision context.

#### High-Privilege Capabilities (explicit allowlisting required)

The following capability tags require explicit operator allowlisting via config or CLI; default behavior is deny even for `trusted` agents:

- `shell.exec`, `shell.spawn`, `shell.eval` — arbitrary command execution.
- `fs.write`, `fs.delete` — non-sandboxed filesystem mutation.
- `net.connect`, `net.listen` — network egress/ingress (an agent reaching outside the local machine).
- `crypto.sign`, `crypto.decrypt` — operations on user-owned key material.
- `secrets.read` — reading from a secret store (e.g., system keychain).

The list is extensible per-deployment via `/etc/zorn-mesh/high-privilege-capabilities.toml` or `${ZORN_CONFIG_DIR}/high-privilege-capabilities.toml`. Adding to the list is operator action; removing is also operator action and emits a `policy_change` audit event.

### Revocation

- **Per-agent revocation:** `zorn revoke <agent_id> [--reason <text>]` immediately disconnects the agent's UDS connection, marks `agents.status = 'revoked'`, and refuses re-registration until manual `zorn unrevoke`. In-flight envelopes from the revoked agent are NACKed with `agent_revoked`.
- **Per-capability revocation:** `zorn revoke-capability <agent_id> <capability_ref>` removes the capability from the agent's AgentCard registration without disconnecting. The agent may still operate other capabilities.
- **Token revocation:** since tokens have ≤5-minute TTL, blanket revocation is achieved by rotating the daemon's session signing key (`zorn rotate-session-key`). All outstanding tokens become invalid; agents re-request transparently.
- **Key rotation:** signed-envelope key rotation is per-agent (`zorn agent rotate-key <agent_id>`); the daemon enforces a grace window where both old and new keys verify successfully (default 60 s).

### Anti-Replay

- Every signed envelope MUST carry `security.nonce` (UUIDv7 or random 128-bit value).
- The daemon maintains a per-agent replay-cache of recently-seen nonces, sized at `anti_replay_window_ms` (default 600 000 ms = 10 minutes; per-capability override permitted).
- Nonce reuse within the window → `NACK replay_detected` and audit_log entry.
- Nonces older than the window are not checked (cache eviction); senders MUST NOT use stale timestamps as nonces.

### Audit Discipline

- **Every authorization decision** (permit AND deny) for capabilities tagged `requires_audit = true` writes an `audit_log` entry. By default, all `requires_token = true` and all high-privilege capabilities are audit-tagged.
- **Every trust-level transition** writes an audit entry.
- **Every key rotation, registration, capability change** writes an audit entry.
- Audit entries are tamper-evident: each entry includes a hash chain reference to the prior entry's hash. Verification is offline-replay via `zorn audit verify`.
- Audit log is **never truncated** by retention rules within the 30-day default window (Cat 4); operators wanting longer retention configure per-deployment.

### Confused-Deputy and Prompt-Injection Mitigations

The daemon cannot prevent prompt injection — that's an application-layer concern. But it provides primitives consuming agents can use:

- **`zorn_envelope.identity.trust_level`** is propagated through every routing hop. A `trusted` agent receiving a message originated by an `untrusted` agent sees the original `untrusted` label even if the message was relayed.
- **`zorn_envelope.routing.origin_chain`** records the full chain of `agent_id`s the message traversed before arriving at the current consumer. Cycle detection: if an `agent_id` appears twice in the chain, daemon refuses delivery with `routing_cycle_detected` (prevents a malicious agent from laundering trust by bouncing through a trusted intermediary).
- **High-risk-capability operator confirmation:** capabilities tagged `requires_human_approval = true` cause the daemon to halt the invocation, emit a notification to the dashboard (or CLI prompt if no dashboard), and wait for explicit operator approval before proceeding. Inspired by MCP's tool-approval flow. Useful for shell.exec et al.

### Sandboxing & Process Isolation

- The daemon does NOT sandbox agents — agents are user-spawned processes outside the daemon's lifecycle. Sandboxing is the operator's responsibility (e.g., run an agent under `firejail`, `bubblewrap`, or macOS `sandbox-exec`).
- The daemon DOES isolate its own privilege: `zorn-meshd` runs as the user's UID, never as root. Setuid invocation is forbidden.
- Daemon-spawned auxiliary processes (test harness, debug control socket consumers) inherit the daemon's UID/GID — never escalate.

### Forbidden Patterns

- ❌ **Trusting `identity` block on the WS-gateway path for authz decisions.** Gateway-injected session token only.
- ❌ **Long-lived capability tokens (>5 minutes).**
- ❌ **Authorization decisions made at the SDK.** Agents are clients of the policy; the daemon owns the decision.
- ❌ **Hard-coded trust assignments** in source code. Trust comes from registration + operator action only.
- ❌ **Bypassing the audit log for "internal" capabilities.** Internal agents (`trust_level = system`) audit too.
- ❌ **Storing the daemon's session signing key on disk in plaintext.** Use the OS keychain (macOS Keychain, Linux libsecret) or generate per-startup if no keychain available (token TTL caps risk).
- ❌ **Exposing the debug control socket without `ZORN_DEBUG=1`** (Cat 1) and outside development environments.
- ❌ **Logging or tracing capability token contents, signatures, or nonces.** Redaction wrappers (Cat 3) MUST cover these fields.
- ❌ **Daemon running as root or with elevated capabilities** (POSIX `CAP_*` capabilities, macOS entitlements). User-level only.
- ❌ **Trusting `pid` for authz across the auto-spawn lifecycle.** PIDs are recycled; rely on the durable AgentCard.id binding, with `pid` as supplementary signal only.
- ❌ **Implicit allow-by-omission.** A capability without a `trust_level_required` declaration defaults to `system`, not `untrusted`. Misconfiguration fails closed.

### Conformance Test Anchors

Every "MUST"/"MUST NOT" rule above is anchored to a fixture under `/conformance/security/`:

- `security/peercred-binding/` — UDS connection's `(uid, gid, pid)` extracted via SO_PEERCRED matches the daemon-recorded identity; spoofing attempt (sending different identity in envelope vs kernel-reported) → NACK.
- `security/agentcard-id-mismatch/` — envelope claiming a different `AgentCard.id` than registration → NACK and connection close.
- `security/agentcard-uid-change/` — re-registration of an `AgentCard.id` from a different `(uid, gid)` → trust downgrade + audit event.
- `security/capability-token-ttl/` — token >5-minute TTL rejected at issuance; expired token rejected at use; rotated session key invalidates outstanding tokens.
- `security/signed-envelope-verification/` — valid signature accepted; tampered envelope rejected; replayed nonce rejected; key rotation grace window honored.
- `security/trust-level-default-deny/` — `untrusted` agent invoking a `trust_level_required = "trusted"` capability → NACK; `trusted` agent invoking same → permit.
- `security/high-privilege-allowlist/` — `shell.exec` invocation by trusted-but-not-allowlisted agent → NACK; allowlisted agent → permit; allowlist removal → subsequent invocation NACK.
- `security/revocation-immediate/` — `zorn revoke` disconnects the agent within 1 s; in-flight envelopes NACKed with `agent_revoked`; re-registration refused.
- `security/origin-chain-cycle/` — message routed through agent A → B → A → daemon rejects with `routing_cycle_detected`.
- `security/human-approval-flow/` — capability tagged `requires_human_approval` halts invocation; operator approves via dashboard/CLI; invocation proceeds; operator denies; NACK.
- `security/audit-hash-chain/` — audit_log entries chain by hash; tampering with any entry detected by `zorn audit verify`.
- `security/secret-redaction/` — capability token, signature, nonce, agent private key NEVER appear in span attributes, log fields, or error messages (regex CI gate over all SDK + daemon log output).
- `security/ws-gateway-identity-not-trusted/` — envelope on WS-gateway path with self-claimed `uid: 0` is treated as informational only; authz uses gateway session token.
- `security/no-root-elevation/` — daemon running as root → refuses to start with explicit error; setuid invocation → refuses.

---

## Observability

OpenTelemetry is the only observability stack (Cat 1). This category specifies what gets instrumented, with what semantic conventions, and at what cardinality.

### Three Signal Types — All Mandatory

Every component (daemon, SDKs, CLI, dashboard backend) emits all three OTel signals:

- **Traces** — every envelope produces a span tree from ingress to delivery (or DLQ). Trace context propagates through the protocol-tier partition (Cat 2) via `traceparent` in both envelope and control frames.
- **Metrics** — counters, histograms, gauges for every routing decision, every queue depth, every retry attempt. RED method (Rate / Errors / Duration) for capabilities; USE method (Utilization / Saturation / Errors) for resources (writer inbox, idempotency cache, WAL size).
- **Logs** — structured records via the OTel logs API. Stdout/stderr is forbidden in library code (Cat 3) post-bootstrap; bootstrap-phase logs use the `ZORN_BOOTSTRAP:` stderr protocol (Cat 3).

### Span Hierarchy — required structure for every envelope

A single envelope produces a strictly-shaped span tree:

```
zorn.sdk.publish (root in producer)
└─ zorn.sdk.serialize
└─ zorn.sdk.transport.write
   └─ zorn.daemon.ingress              ← daemon receives
      ├─ zorn.daemon.frame.discriminate
      ├─ zorn.daemon.envelope.validate (3 child spans for the 3 layers)
      │  ├─ zorn.daemon.validate.jsonrpc
      │  ├─ zorn.daemon.validate.envelope
      │  └─ zorn.daemon.validate.capability
      ├─ zorn.daemon.authz              ← authorization decision (Cat 5)
      ├─ zorn.daemon.idempotency.lookup
      ├─ zorn.daemon.persist             ← writer transaction
      │  └─ zorn.daemon.db.commit
      └─ zorn.daemon.route               ← routing decision
         └─ zorn.daemon.deliver           ← per-target span
            └─ zorn.daemon.transport.write
               └─ zorn.sdk.handle (in consumer)
                  └─ zorn.sdk.deserialize
```

Spans MUST chain via `parent_id`. Producer-consumer linkage uses `traceparent` propagation. The 3-layer validation produces 3 named child spans even on success (cheap, traceable).

### Span Naming — Locked Convention

All span names follow `zorn.<component>.<operation>`:

- Component names (closed set; new components require spec amendment): `sdk`, `daemon`, `cli`, `dashboard`, `gateway`.
- Operation names use snake_case (matching wire format).
- Sub-operations under a parent component use dot separation: `daemon.envelope.validate`, `daemon.db.commit`, `daemon.transport.write`.

Span names are **low-cardinality identifiers**, not message content. Per-message specifics live in attributes.

### Span Attributes — Semantic Conventions

#### Standard OTel attributes (use where they exist)

- `service.name`, `service.version`, `service.instance.id` (resource attributes set once at SDK init)
- `code.namespace`, `code.function`, `code.filepath`, `code.lineno`
- `error.type`, `exception.type`, `exception.message`, `exception.stacktrace`
- `messaging.system = "zorn-mesh"` (locked value)
- `messaging.operation` (`publish` | `receive` | `process`)
- `messaging.destination.name` = capability_ref or topic name
- `messaging.message.id` = `message_id`
- `messaging.message.conversation_id` = `correlation_id`

#### Zorn-specific attributes (prefix `zorn.*`, locked names)

| Attribute | Type | Where | Notes |
|---|---|---|---|
| `zorn.envelope.version` | string | every span | from `envelope_version` field |
| `zorn.envelope.frame_type` | string | every span | `envelope` or `control` |
| `zorn.envelope.delivery_mode` | string | every envelope span | `at_least_once` / `at_most_once` |
| `zorn.envelope.attempt` | int | retry-loop spans | 1-indexed |
| `zorn.envelope.size_bytes` | int | ingress + serialize spans | post-validation byte size |
| `zorn.agent.from_id` | string | every span | originating AgentCard.id |
| `zorn.agent.to_id` | string | routing/delivery spans | target AgentCard.id |
| `zorn.agent.trust_level` | string | authz spans | `system`/`trusted`/`untrusted` |
| `zorn.capability.ref` | string | every envelope span | e.g., `code_edit@2.1.0` |
| `zorn.capability.version` | string | every envelope span | semver |
| `zorn.routing.pattern` | string | every envelope span | `request`/`response`/`event`/`stream` |
| `zorn.routing.origin_chain_depth` | int | route spans | length of `origin_chain` |
| `zorn.idempotency.cached` | bool | idempotency.lookup spans | true if cache hit |
| `zorn.idempotency.synthetic` | bool | idempotency.lookup spans | true if SDK-synthesized |
| `zorn.db.tx_age_ms` | int | persist spans | inbox-arrival → commit elapsed |
| `zorn.db.busy_timeout_hits` | int | persist spans | counter |
| `zorn.frame.opcode` | string | control-frame spans | `ack`/`nack`/`stream_end`/`ping`/`pong`/`capability_probe` |
| `zorn.frame.nack_code` | int | nack spans | registered error code from `/docs/jsonrpc-error-codes.md` |
| `zorn.transport` | string | every span | `uds` / `ws_gateway` |
| `zorn.gateway.verified` | bool | gateway spans | from `gateway_verified` flag |
| `zorn.replay.session_id` | string | replay spans | UUIDv7 |
| `zorn.replay.origin` | string | replay spans | always `zorn-replay` |
| `zorn.daemon.actor` | string | actor-internal spans | `WalKeeper`/`RetentionWorker`/`IdempotencyPruner`/`writer` |

**Forbidden attribute content:**
- ❌ Capability payload bodies (privacy + payload size).
- ❌ Idempotency key contents (opaque + may contain identifying material).
- ❌ Capability tokens, signatures, nonces, private keys (Cat 5 redaction).
- ❌ Env var values (Cat 3 redaction).
- ❌ User filesystem paths beyond the daemon's owned directories (e.g., never `/Users/<name>/...` from a `fs.write` capability invocation; the `path` attribute, if present, is hashed: `zorn.fs.path_hash` instead).

### Cardinality Discipline

Trace and metric cardinality explosions are the #1 production scar in agent meshes — high-cardinality attributes (per-message-id, per-trace-id) produce metric series counts in the millions and crash collectors.

**Rules:**

- **Never use `message_id`, `trace_id`, `correlation_id`, or `idempotency_key` as a metric label.** They belong in trace attributes only.
- **Capability ref** is acceptable as a metric label IF the deployment's capability count is bounded (<1000). Operators with larger fleets must drop `zorn.capability.ref` from metric labels and rely on traces.
- **`agent_id`** as a metric label is bounded by the registered agent count (typically <50 on a developer machine). Acceptable.
- **`zorn.frame.nack_code`** is a small enum from the registered error code registry. Acceptable.
- **`zorn.transport`** has 2 values. Acceptable.

The daemon's metrics emitter MUST use a configurable cardinality cap (`ZORN_METRICS_MAX_LABEL_VALUES`, default 10000); exceeding the cap drops new label combinations and emits a `metrics.cardinality_exceeded` warning span.

### Required Metrics

Every emitter (daemon, SDK, CLI, dashboard) registers these metrics. Names follow OTel semantic conventions where they exist; Zorn-specific names use `zorn.*`.

#### Counters

- `zorn.envelope.published` (per `routing.pattern`, `capability.ref`)
- `zorn.envelope.delivered` (per `to_agent_id`, `capability.ref`)
- `zorn.envelope.nacked` (per `frame.nack_code`)
- `zorn.envelope.dlq` (per `capability.ref`)
- `zorn.envelope.replayed` (per `replay.session_id` — short-lived, sessions tear down)
- `zorn.authz.denied` (per `frame.nack_code` mapped to `authz_denied` reasons)
- `zorn.idempotency.cache_hit` / `zorn.idempotency.cache_miss`
- `zorn.frame.received` (per `frame_type`, `transport`)

#### Histograms

- `zorn.envelope.latency_ms` (per `routing.pattern`, `capability.ref`) — ingress → delivery wall clock
- `zorn.db.tx_duration_ms` — single-writer transaction duration; SLO 50 ms (Cat 4)
- `zorn.daemon.ingress.queue_wait_ms` — inbox queue depth in time units
- `zorn.envelope.size_bytes` — distribution of payload sizes
- `zorn.handshake.duration_ms` — `initialize` round-trip

#### Gauges

- `zorn.daemon.writer.inbox_depth`
- `zorn.daemon.connections.active`
- `zorn.daemon.agents.registered` (per `trust_level`)
- `zorn.daemon.idempotency.cache_size` (rows)
- `zorn.daemon.dlq.size` (rows)
- `zorn.daemon.wal.size_bytes`
- `zorn.daemon.db.size_bytes`

### Required Logs

OTel logs API only (post-bootstrap; bootstrap uses `ZORN_BOOTSTRAP:` stderr protocol — Cat 3).

Every log record carries:

- The current span's `trace_id` and `span_id` (auto-attached by OTel SDK).
- A `severity_text` of `TRACE`/`DEBUG`/`INFO`/`WARN`/`ERROR`.
- A `body` that is a structured map, not a formatted string.
- Resource attributes from the emitter (service.name, etc.).

**Required log events** (every emitter):

- `agent.registered` — agent_id, trust_level, uid, gid, AgentCard.version
- `agent.revoked` — agent_id, reason, operator_id
- `auth.denied` — agent_id, capability, frame.nack_code, reason
- `daemon.shutdown_initiated` — signal, budget_ms (Cat 3 ops discipline)
- `daemon.shutdown_complete` — duration_ms, drained_count, dlq_added_count
- `daemon.degraded_entered` — reason (`disk_full` / `recovery_blocked` / etc.)
- `daemon.degraded_recovered` — reason
- `key.rotated` — agent_id, old_key_id, new_key_id, grace_window_ms
- `migration.applied` — version, duration_ms
- `vacuum.invoked` — kind (`incremental` / `full`), duration_ms, pages_freed
- `replay.session_started` / `replay.session_ended` — session_id, count, duration_ms

### W3C tracecontext Propagation

- `traceparent` and `tracestate` are first-class envelope members (Cat 2). They are also schema-mandated fields in control_frames (Cat 2 — preserves span continuity across NACKs).
- SDK auto-spawn shim propagates trace context from the host application's current span (if any) into the daemon's ingress span — auto-spawn is part of the publish trace, not a separate trace.
- The bootstrap log contract (Cat 3) carries no `traceparent` for daemon-startup-phase logs because OTel SDK is not yet initialized; once initialized, all subsequent logs and spans participate in the host's trace.

### Sampling

- **Default sampling rate:** `ZORN_TRACE_SAMPLE_RATIO = 1.0` (always sample) for local-developer use.
- **Production deployments** (operators running multiple agents over long-lived sessions) override to `0.1` or below.
- **Always-sample categories** (regardless of ratio): authz denials, NACKs, DLQ moves, replay sessions, key rotations, every audit-tagged capability invocation.
- **Per-trace consistency:** sampling decision is made at the root span and propagated through `traceparent.flags`. A sampled trace stays sampled across the full envelope flow, even if the rate is reduced mid-flow.

### Dashboard Surfaces (operator-facing)

The optional Next.js dashboard subscribes to the daemon's gateway and surfaces:

1. **Live agent topology** — graph of registered agents, capabilities, current connections.
2. **Message timeline** — per-trace or per-correlation-id timeline of envelope flow.
3. **DLQ panel** — primary surface (Cat 4); message_id + reason_code + first-failure-trace link + replay button.
4. **Authz audit feed** — denials, trust transitions, key rotations.
5. **Health panel** — heartbeats, error rates, queue depths, WAL/DB size, daemon mode (running/degraded/recovery_blocked).
6. **Trace explorer** — query traces by trace_id, agent_id, capability_ref, time range.
7. **Replay tool** — operator-initiated replay against named agents/topics with the soft-hold coordination from Cat 4.

The dashboard never opens the SQLite file directly; it consumes data via the gateway's WebSocket using read-only queries the daemon exposes. CSRF token + session token discipline (Cat 2/5) applies.

### OTel Collector — In-Tree Test Collector + Local-Developer Loopback

- **Test collector config** at `/test-infra/otel-collector.yaml` is launched by the shared test harness (Cat 3). Integration tests assert on span structure via this collector.
- **Local-developer Prometheus scrape endpoint** on the gateway (Cat 1 commitment) — observable via `curl http://127.0.0.1:<gateway_port>/metrics` with the same CSRF/session-token discipline as the dashboard.
- **No collector required for happy-path local use** — OTel SDK can buffer and drop if no collector is reachable. Developer machines work without an external collector; the Prometheus endpoint provides observability.

### Forbidden Patterns

- ❌ **Stdout/stderr printing in library code post-bootstrap.** OTel logs API only (Cat 3 reaffirmation).
- ❌ **Custom log levels** beyond TRACE/DEBUG/INFO/WARN/ERROR.
- ❌ **High-cardinality attributes as metric labels** (message_id, trace_id, correlation_id, idempotency_key).
- ❌ **Logging payload bodies, secrets, tokens, signatures, nonces, or private keys.** Cat 3/5 redaction MUST cover.
- ❌ **Span names that include identifiers** (e.g., `zorn.daemon.deliver.agent-12345`). Identifiers belong in attributes.
- ❌ **Vendor-specific instrumentation libraries** (Datadog tracer, New Relic agent, Honeycomb beelines). OTel-only (Cat 1).
- ❌ **Skipping span emission on success paths** to "save volume." If the operation matters enough to fail, it matters enough to span.
- ❌ **Direct collector configuration in source code.** Configuration via `OTEL_EXPORTER_OTLP_ENDPOINT` env var (Cat 3) only.
- ❌ **Error-only logging** (omitting INFO/DEBUG). Successful operations need observability too.
- ❌ **Multiple OTel SDKs in the same process** (e.g., one per third-party library). One SDK init per process.

### Conformance Test Anchors

Every "MUST"/"MUST NOT" rule above is anchored to a fixture under `/conformance/observability/`:

- `observability/span-tree-shape/` — single envelope produces the canonical span tree (15+ spans); parent_id chains correctly; cross-process spans link via traceparent.
- `observability/control-frame-traceparent/` — NACK control frame carries the originating envelope's traceparent; span continuity preserved (Cat 2 reaffirmation).
- `observability/attribute-coverage/` — every required attribute is present with the correct type on every applicable span; missing attribute fails CI.
- `observability/cardinality-cap/` — exceeding `ZORN_METRICS_MAX_LABEL_VALUES` drops new combinations and emits warning span; existing series continue.
- `observability/forbidden-attributes/` — payload bodies, secrets, tokens, signatures NEVER appear in span attributes (regex CI gate).
- `observability/sampling-consistency/` — sampling decision at root span propagates through full envelope flow; mid-flow consumers don't re-sample.
- `observability/always-sample-overrides/` — authz denials and DLQ moves sampled at 1.0 even when global ratio is 0.001.
- `observability/bootstrap-log-format/` — daemon startup emits valid `ZORN_BOOTSTRAP:` JSON; SDK re-emits as OTel log records post-init (Cat 3 reaffirmation).
- `observability/required-metrics-emitted/` — fresh daemon under synthetic load emits every required counter/histogram/gauge; missing metric fails CI.
- `observability/required-logs-emitted/` — every required log event (agent.registered, auth.denied, etc.) is emitted at the right severity with the right body schema.
- `observability/no-stdout-stderr-post-bootstrap/` — daemon and SDK process stdout/stderr is empty after bootstrap completes (regression test for accidental println).
- `observability/dashboard-csrf/` — dashboard request without session token → gateway rejects; with session token → permits (Cat 5 reaffirmation).
- `observability/test-collector-roundtrip/` — integration test launches test collector, sends envelope, asserts spans and metrics are received with expected attributes.

---

## Testing Discipline

This category specifies the testing pyramid above the conformance fixtures named in Cats 1–6. Conformance fixtures answer "does this rule hold?" — Category 7 specifies how the team writes, runs, and interprets the rest of the test suite.

### Test Pyramid

Five layers, each with a distinct purpose, owner, and gate. A test that fits multiple layers belongs in the lowest applicable one.

```
┌──────────────────────────────────────────────┐
│  L5: Manual exploratory (operator dogfood)   │  weekly
├──────────────────────────────────────────────┤
│  L4: Chaos packs (per-language + cross-SDK)  │  nightly
├──────────────────────────────────────────────┤
│  L3: Conformance (cross-SDK protocol-level)  │  every PR
├──────────────────────────────────────────────┤
│  L2: Integration (single-SDK + daemon)        │  every PR
├──────────────────────────────────────────────┤
│  L1: Unit (per-language, no daemon)           │  every commit
└──────────────────────────────────────────────┘
```

#### L1 — Unit (per-language, no daemon)

- Owner: the language SDK team (or daemon team for daemon-internal modules).
- Runtime: <100 ms per test; full suite <30 s.
- No I/O, no daemon process, no SQLite file (in-memory `:memory:` permitted for SQL-helper tests).
- **Coverage gate:** every PR must maintain or improve unit-test line coverage. Initial floor: 70% on every package; targets ratchet up automatically as coverage grows. Reduction below the prior commit's coverage fails the PR.
- File layout:
  - Rust: `src/<module>/tests.rs` (`#[cfg(test)] mod tests`).
  - TypeScript: `src/<module>.test.ts` colocated with source.
  - Python: `tests/unit/test_<module>.py`.

#### L2 — Integration (single-SDK + real daemon)

- Owner: the language SDK team.
- Runtime: <30 s per test; full suite <5 min.
- Use the **shared test harness** (`zorn-test-harness` binary, Cat 3) via the per-language adapter. No SDK rolls its own daemon launcher.
- Real SQLite, real UDS, real envelope flow. Mocks of the daemon are forbidden — the harness IS the daemon.
- File layout:
  - Rust: `tests/integration/*.rs` (cargo's `tests/` directory).
  - TypeScript: `tests/integration/*.test.ts` with `bun test --test-name-pattern integration`.
  - Python: `tests/integration/test_*.py` with `pytest -m integration`.

#### L3 — Conformance (cross-SDK)

- Owner: a dedicated small "Protocol Conformance" rotation; not any single SDK team.
- Runtime: <10 min for full corpus across all three SDKs in parallel.
- Driven by `/conformance/<category>/` fixtures named in Cats 1–6 (frame-discriminator, idempotency, handshake, agentcard-stability, timestamp, ttl-retry, streaming, identity-platform, ws-gateway, translation-wall, canary-field, control-frame-traceparent, persistence/*, security/*, observability/*).
- Each fixture is a JSON or TOML scenario file consumed identically by all three SDK adapters.
- Single CI workflow `conformance.yml` runs the corpus against Rust + TS-Bun + Python in parallel and asserts byte/value equivalence.
- **Conformance failures are triaged by ALL THREE SDK leads jointly** within 4 hours of CI failure (see Failure Attribution below).

#### L4 — Chaos packs (per-language + cross-SDK)

- Owner: SRE-style rotation; the language SDK team contributes pack-specific tests; the rotation owns the runner.
- Runtime: nightly job, <2 hours total.
- Per-language packs at `tests/chaos/{rust,python,typescript}/` (Cat 3) covering language-idiomatic failure modes.
- Cross-SDK pack at `tests/chaos/cross_sdk/` covering wire-level fault injection, daemon kill scenarios, disk-full + crash, busy_timeout cascade, replay-during-prune (Cat 4 anchors).
- Failure → automatic GitHub issue creation tagged `chaos-failure` with the failing scenario name + reproducer command.
- **Python cancellation suite ships first** and runs every PR as part of L2, not just nightly L4. (Cat 3 — highest 2am-page risk.)

#### L5 — Manual exploratory

- Owner: the whole team on a rotation; everyone takes a turn.
- Runtime: weekly, ~30 min.
- Run a real workflow end-to-end: spawn three real agents (Claude desktop config + a local Python agent + the CLI), exercise the dashboard, replay messages, force a crash, restore from backup, observe traces. Notes go into a `/tests/exploratory/<date>.md` log.
- Bugs found here are filed normally; the value is the *exposure* of failure modes the automated tiers miss.

### The Shared Test Harness Contract

The `zorn-test-harness` binary (Cat 3) is the only daemon-launcher used by L2/L3/L4 tests. It exposes a stable JSON-over-stdin/stdout protocol:

**Operation guarantees:**

- `start` returns within 200 ms (matches Cat 1's auto-spawn invariant) and emits `{"event":"ready","socket":"<path>"}` once the UDS is listening.
- `stop` performs graceful shutdown within 1 s (compressed from Cat 3's 10 s production budget — tests don't drain real traffic).
- `inject_fault` is idempotent within a single harness instance — repeated injection of the same fault overrides the prior, never composes.
- `assert_delivered` is the canonical wait primitive; never use `sleep()` to wait for delivery.
- The harness exits 0 only if its own internal invariants pass — a harness exit ≠ 0 with no test failure indicates a harness bug, not a code-under-test bug.

**Forbidden in test code:**

- Calling `zorn-meshd` directly via `Command::new` / `Bun.spawn` / `subprocess.Popen` to start a daemon.
- Sharing a daemon across tests in the same suite (every test gets a fresh harness for isolation).
- `sleep()` waits for state transitions — use `assert_*` primitives or `wait_for` with explicit predicate + timeout.
- Hardcoded UDS paths, port numbers, or PIDs — use the harness's emitted `socket` event.

### Failure Attribution Discipline

The hardest test problem in a polyglot mesh is *which team owns the failure*. The protocol:

1. **Fail with a routing tag.** Every test failure, regardless of layer, prints a single-line tag in the form `[FAIL-OWNER: <team>] <fixture-name>` immediately before the assertion stack trace. CI tools route the failure to the named team's on-call.
   - L1/L2 failures: tag = the SDK or daemon team.
   - L3 conformance failures: tag = `[FAIL-OWNER: triage]` (jointly owned; see below).
   - L4 chaos failures: tag = `[FAIL-OWNER: <component>]` based on which span emitted the error.
2. **Conformance failure triage protocol:**
   - Within 1 hour of CI failure, whoever is on the Conformance Rotation opens the failure: examines whether all three SDKs fail (likely daemon or fixture bug) or only one (likely SDK bug).
   - Within 4 hours, the triage leads file the failure against the responsible team OR mark it `[fixture-bug]` if the test itself is wrong.
   - A conformance failure that is unattributed for >24 hours is a release blocker; CI marks it `[escalate]` and the engineering lead is paged.
3. **No "flaky" without quarantine.** Marking a test flaky requires opening a tracking issue, moving the test to `tests/flaky/` (which still runs but doesn't gate the PR), and assigning an owner with a deadline. Tests in `flaky/` for >2 weeks are deleted.

### Determinism

- **Time:** every test that asserts on time uses an injected clock (Rust: `tokio::time::pause`; Bun: `Bun.test.fakeTimers`; Python: `freezegun` or pytest's `monkeypatch` on `datetime.now`). Wall-clock-dependent assertions are forbidden.
- **Randomness:** every random source is seeded. Rust: `rand::SeedableRng::seed_from_u64`; TS: `Math.random` is forbidden in tests, use `seedrandom` or the harness's deterministic PRNG; Python: `random.seed(0)` at test setup.
- **Concurrency:** tests that exercise concurrent paths use the harness's `inject_fault` for ordering, not `tokio::time::sleep`. Race conditions are reproduced via fault injection, not luck.
- **UUIDs:** envelope IDs in tests use UUIDv7 with a fixed timestamp + counter (the harness provides a `next_uuid` operation). UUIDv4 in tests is forbidden — non-reproducible failures.

### Coverage Discipline

- Per-package floor: **70%** line coverage on day one of code arrival.
- Auto-ratchet: every PR's coverage floor rises to the prior `main`'s coverage (rounded down to nearest integer percent). PRs may not lower it.
- **Branch coverage** target on routing-critical modules (daemon's router, idempotency gate, control-frame discriminator): **90%**.
- Coverage tools:
  - Rust: `cargo-llvm-cov` (or `cargo-tarpaulin` as fallback).
  - TS: Bun's built-in `bun test --coverage`.
  - Python: `coverage.py` via pytest-cov.
- Coverage report is a CI artifact, not a gate-blocker on its own — the gate is the auto-ratchet floor.

### Property-Based Testing

Property-based testing is mandatory for:

- **Envelope serialization round-trip** — for any TypeBox-generated value, `deserialize(serialize(v)) == v` (Rust `proptest`, TS `fast-check`, Python `hypothesis`).
- **Idempotency hash determinism** — across all three SDKs, the synthetic key for a fixed `(message_id, agent_id, capability_ref)` is byte-identical.
- **Wire format byte-equivalence** — Protobuf encoding of a fixed envelope is byte-identical across `prost` (Rust), `@bufbuild/protobuf` (TS), `betterproto` (Python).
- **Frame discriminator** — for any random byte sequence, the discriminator either parses to a valid frame or rejects with a registered error code (no panics, no UB).

Property tests run as part of L1 (unit) on every commit. Counterexamples discovered by property tests are saved as regression fixtures.

### Performance / Load Testing

Load testing is **not part of the regular test suite** — it runs on dedicated infrastructure on a weekly cadence with results published to the dashboard's performance panel.

- **k6 scenarios** at `/tests/perf/k6/` drive the harness via the gateway.
- **Tracked metrics:**
  - p50 / p95 / p99 envelope latency under sustained 1000 msg/s synthetic load.
  - Sustained throughput before writer inbox saturates.
  - DB transaction latency at p95 / p99.
  - WAL growth rate vs checkpoint frequency.
- **Regression alerting:** week-over-week >20% degradation in any tracked metric files an automatic issue.
- **No perf-test results gate PRs.** Perf is a trend, not a gate.

### Mutation Testing (advisory, not gate)

- Rust: `cargo-mutants` runs nightly on the daemon's routing module. Surviving mutants are reviewed weekly; the goal is killed-mutant ratio >85% on the routing path.
- Other languages: mutation testing is permitted but not mandated.
- Mutation testing is **never a gate** — false positives outweigh value if it blocks PRs.

### Snapshot / Golden Tests

- Snapshot tests are permitted for: serialized envelope formats, generated SDK code from `.proto` files, dashboard rendered components.
- Snapshots live in `__snapshots__/` directories beside the test source.
- **Snapshot updates require human review.** CI rejects PRs that update snapshots without a corresponding test-file diff explaining the change.
- Snapshots that drift across SDK versions without a corresponding spec-version bump are a release blocker.

### Forbidden Patterns

- ❌ **Mocking the daemon.** Use the test harness. Mocked daemon → wrong invariants tested.
- ❌ **Mocking SQLite** in integration tests. Use a real `:memory:` or temp-file DB.
- ❌ **`sleep()` for state transitions.** Use harness's `assert_*` / `wait_for` primitives.
- ❌ **Wall-clock-dependent assertions** without an injected clock.
- ❌ **Shared daemon across tests** in the same suite. Each test gets a fresh harness.
- ❌ **Hardcoded socket paths / ports / PIDs.**
- ❌ **`UUIDv4` in test fixtures or assertions.** Use UUIDv7 with deterministic time + counter.
- ❌ **Snapshot updates without a corresponding code/spec change.**
- ❌ **`#[ignore]` / `xtest` / `xit` / `pytest.mark.skip` without a tracking issue and deadline.**
- ❌ **Tests that depend on network egress.** Outbound connections are blocked in CI.
- ❌ **Tests that depend on a specific OS** without a matching `cfg!`/platform check at the test boundary. Cross-platform parity is enforced by the matrix CI run.
- ❌ **Manual `Command::new`/`Bun.spawn`/`subprocess.Popen` of `zorn-meshd`.** Harness only.
- ❌ **Conformance fixtures hardcoded in a single language's test suite.** They live in `/conformance/` and are consumed by all three SDKs.
- ❌ **Marking a test flaky as a substitute for fixing it.** Use the quarantine + 2-week-deadline protocol.

### Conformance Test Anchors (meta — testing the testing)

Yes, even the testing discipline has conformance anchors. These verify the discipline itself isn't drifting:

- `meta/coverage-ratchet/` — a no-op PR that lowers a per-package coverage by 1% must fail; raising it must pass.
- `meta/harness-protocol-stability/` — golden recording of the harness's stdin/stdout protocol; any change to the JSON event shape requires updating this fixture and bumping the harness's version.
- `meta/property-test-determinism/` — property tests run with a fixed seed produce identical counterexamples on every CI run.
- `meta/no-sleep-in-tests/` — regex CI gate that fails any test file containing `sleep(`, `Thread.sleep`, `setTimeout(`, `time.sleep(` outside an explicitly tagged `@perf-test` block.
- `meta/failure-attribution-tag/` — every test failure prints a `[FAIL-OWNER:]` line; tested by injecting a synthetic failure and asserting on stdout.
- `meta/flaky-quarantine-deadline/` — tests in `tests/flaky/` carry a `// DEADLINE: <ISO-date>` comment; CI fails if any deadline is past.

---

## Workflow, Style, and Don't-Miss Rules

This is the meta category: git workflow, commit conventions, code-review gates, release discipline, plus a curated index of cross-category rules an AI agent or human MUST NOT miss.

### Branch & PR Workflow

- **Trunk-based.** `main` is always releasable. No long-lived feature branches; if a feature won't merge in <1 week, split it into smaller PRs behind a feature flag.
- **Branch naming:** `<author>/<short-slug>` (e.g., `nebrass/idempotency-prune-fix`). No issue numbers in branch names — they belong in commit messages and PR descriptions.
- **PR scope:** every PR addresses one concern. Mixed-concern PRs are split before merge. A PR that touches files in >2 SDKs must be split unless it's a synchronized envelope-version bump.
- **PR review:** every PR has at least one human review approval. Even AI-authored PRs require a human reviewer before merge.
- **PR description requirements:**
  - "What" — one paragraph describing the change.
  - "Why" — links to the issue, ticket, or context.
  - "How tested" — names the conformance fixture(s) or unit tests that verify the change.
  - "Spec impact" — names any category in this document the change modifies; spec changes MUST be in the same PR as the implementation.
- **Merge strategy:** **squash and merge.** No merge commits in main. The squash commit message follows the Conventional Commits format (below).

### Commit Convention

Conventional Commits format: `<type>(<scope>): <subject>`

- **Types:** `feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore` | `perf` | `build` | `ci` | `revert`
- **Scope** (optional but encouraged): `daemon` | `sdk-rust` | `sdk-ts` | `sdk-py` | `cli` | `dashboard` | `harness` | `proto` | `migrations` | `conformance`
- **Subject:** ≤50 chars, imperative mood, no period.
- **Body** (for non-trivial changes): wrap 72 chars; explain *what changed* and *why* (the *what* is also in the diff, the *why* is what the body adds). Reference issues with `Refs: #123` or `Closes: #123`.
- **Breaking changes:** `feat(daemon)!: rename agent.heartbeat to agent.ping` — the `!` after type/scope marks the breaker. Body must include `BREAKING CHANGE: <description and migration path>`.
- **Co-authored-by:** when AI authored, add `Co-authored-by: Claude <noreply@anthropic.com>` (or equivalent); humans review the substance, not the authorship line.

### Code-Review Gates

A PR is mergeable only when ALL of the following pass:

1. **CI green:** all conformance fixtures, unit suites, integration suites, type checks, lint checks, and the forbidden-pattern enforcement matrix from Cat 3.
2. **Coverage ratchet** holds (Cat 7).
3. **At least one human review approval.**
4. **No `[FAIL-OWNER: triage]` conformance failures** outstanding from prior PRs.
5. **No spec drift:** if the PR touches a rule in Cats 1–7, the corresponding spec section is updated in the same PR.
6. **Cross-SDK lockstep:** if `sdk_version` (Cat 3) changes, all three SDK packages bump in the same PR.
7. **Migration safety:** schema migrations must include a forward-only test (Cat 4) AND a `migration-mid-checkpoint` chaos fixture pass.
8. **Audit log impact:** PRs that touch security or authorization paths get an extra reviewer from the security-aware rotation.

### Release Discipline

- **Semantic versioning** at two axes (Cat 3): `sdk_version` (lockstep) and `build_version` (per-SDK).
- **Release artifacts:** Rust binary (multi-arch), Bun-published `@zorn/mesh` package, PyPI `zorn-mesh` wheel, `zorn` CLI binary. All four are signed with a release key (Sigstore cosign or equivalent).
- **Release notes** are auto-generated from Conventional Commits between the prior tag and HEAD, then human-edited for clarity. Breaking changes get their own section with migration notes.
- **No silent dependency upgrades.** Lockfile changes that aren't security-driven require explicit reviewer approval.
- **Security advisories:** RUSTSEC, npm audit, or PyPA advisory affecting any pinned dep triggers an out-of-band patch release within 7 days. Lower CVSS may go on the next minor.

### Documentation Discipline

- **The spec (this document) is the source of truth.** Implementation must follow spec; if implementation must diverge, spec is updated in the same PR.
- **Inline doc comments** are required on every public symbol in every SDK (Rust `///`, TS JSDoc, Python docstrings). Coverage gate via `cargo doc --no-deps -D missing_docs`, `tsdoc-required` ESLint rule, and `pydocstyle` in strict mode.
- **External docs** at `/docs/` cover: getting started, agent author guide, capability author guide, operator runbook, troubleshooting. Generated API references live under `/docs/reference/` and are regenerated on every release.
- **Forbidden in docs:** AI-style filler (`✨`, "Let's dive in", "It's worth noting"), unjustified emojis, and time estimates ("this takes ~5 minutes"). Match the user's voice: crisp, precise, every token earns its place.

### Naming Conventions (cross-category index)

- **Files:**
  - Rust: `snake_case.rs`
  - TypeScript: `kebab-case.ts` (or `camelCase.ts` for module-private files; pick one per package and stay consistent)
  - Python: `snake_case.py`
  - Configuration: `kebab-case.toml/.yaml/.json`
- **Identifiers:** snake_case (Rust + Python), camelCase (TypeScript), PascalCase types in all three.
- **Wire format:** snake_case, always (Cat 3).
- **Span names:** `zorn.<component>.<operation>` snake_case (Cat 6).
- **Metric names:** `zorn.<resource>.<dimension>` snake_case (Cat 6).
- **Capability refs:** `<capability_name>@<semver>` (Cat 2).
- **Branch names:** `<author>/<short-slug>` (above).
- **Test files:** colocated with source where possible; `tests/integration/`, `tests/chaos/<lang>/`, `tests/flaky/` directories per Cat 7.

### Don't-Miss Rules — flat index for AI agents and humans

The 20 rules an AI agent implementing a story most often gets wrong if it skims rather than reads:

1. **The bus is a library, not a daemon to install.** Agents call `connect()`; the library auto-spawns the daemon if absent. Never tell users to `start zorn-meshd` manually. (Cat 1)
2. **Two frame tiers, one socket.** Control frames (`frame_type=0x01`, ≤512 B) and envelope frames (`frame_type=0x02`) share the UDS but are validated by separate schemas. Never put ack/nack/stream_end in `routing.pattern`. (Cat 2)
3. **MCP `initialize` is byte-compatible with the spec.** Do not invent a Zorn-specific handshake. (Cat 1, 2)
4. **A2A AgentCard is the identity.** Do not invent a parallel identity model. (Cat 1, 2)
5. **`message_id` is generated once per logical operation and reused across retries.** The synthetic idempotency hash is broken if you regenerate `message_id` per attempt. (Cat 2)
6. **Wire format is snake_case regardless of SDK.** TS callers use camelCase; the SDK translates at serialization. (Cat 3)
7. **No mocks of the daemon.** Tests use the `zorn-test-harness` binary. (Cat 7)
8. **No `sleep()` to wait for state transitions.** Use the harness's `assert_*` / `wait_for` primitives. (Cat 7)
9. **Writer pool is `synchronous=FULL`; reader pool is `NORMAL`.** Per-pool, not blanket. (Cat 1, 4)
10. **ACK only after COMMIT and fsync.** Writer pool's `synchronous=FULL` makes this structurally enforced. (Cat 4)
11. **Idempotency check precedes `pool.begin()`.** Cache hit returns without acquiring a writer transaction. (Cat 4)
12. **Daemon never mutates client envelope fields.** It writes only into its own namespace partitions (`routing.daemon_*`, `tracing.*`). (Cat 2)
13. **All three SDKs must produce byte-identical Protobuf encoding** for a fixed envelope. Verify via the conformance corpus. (Cat 1, 7)
14. **`AgentCard.id` is stable across restarts.** Derive from `ZORN_AGENT_ID` env var or stable hash; never PID/random. (Cat 2)
15. **Default-deny authorization.** New agents are `untrusted`; high-privilege capabilities require explicit allowlisting. (Cat 5)
16. **Capability tokens have ≤5-minute TTL.** Long-lived tokens are forbidden. (Cat 5)
17. **Trace context propagates through control frames.** `traceparent` is schema-mandated even on NACKs. (Cat 2, 6)
18. **No high-cardinality attributes as metric labels.** `message_id`, `trace_id`, `correlation_id`, `idempotency_key` belong in spans only. (Cat 6)
19. **Bootstrap-phase logs use `ZORN_BOOTSTRAP:` stderr protocol.** Library code post-bootstrap uses OTel logs API. Never `println!`/`console.log`/`print` post-bootstrap. (Cat 3)
20. **Spec changes ship in the same PR as implementation.** No silent drift between the spec and the code. (Cat 8 above)

### Cross-Category Forbidden-Pattern Index

For quick reference, all forbidden-pattern sections in this document:

- Cat 1 — Explicit Non-Dependencies
- Cat 2 — Forbidden Patterns (envelope/protocol)
- Cat 3 — Forbidden in Rust / TypeScript / Python (per-SDK)
- Cat 4 — Forbidden Patterns (persistence)
- Cat 5 — Forbidden Patterns (security)
- Cat 6 — Forbidden Patterns (observability)
- Cat 7 — Forbidden Patterns (testing)

When in doubt about whether a pattern is permitted, search this document for `❌`. If a rule is not explicitly forbidden but feels wrong, file a question in the PR rather than guessing.

### Conformance Fixture Index

All conformance fixtures live under `/conformance/<category>/` and are consumed by the cross-SDK conformance runner (Cat 7). Fixture categories:

- `/conformance/frame-discriminator/`, `/conformance/idempotency/`, `/conformance/handshake/`, `/conformance/agentcard-stability/`, `/conformance/timestamp/`, `/conformance/ttl-retry/`, `/conformance/streaming/`, `/conformance/identity-platform/`, `/conformance/ws-gateway/`, `/conformance/translation-wall/`, `/conformance/canary-field/`, `/conformance/control-frame-traceparent/` — Cat 2 anchors
- `/conformance/persistence/*` — Cat 4 anchors
- `/conformance/security/*` — Cat 5 anchors
- `/conformance/observability/*` — Cat 6 anchors
- `/conformance/meta/*` — Cat 7 (testing-the-testing)

### Inclusive Terminology

- Use: allowlist / blocklist, primary / replica, placeholder / example, main branch, conflict-free, concurrent / parallel.
- Do not use: whitelist / blacklist, master / slave, dummy, sanity check.

### Final Forbidden Patterns (workflow-level)

- ❌ **Pushing directly to `main`.** All changes through PRs.
- ❌ **Force-pushing to shared branches.** `main` and any branch with an open PR.
- ❌ **Skipping CI gates.** Even for "trivial" docs PRs, the full CI runs.
- ❌ **Merging your own PR.** Reviewer is always someone else.
- ❌ **Commit messages that are just `wip`, `fix`, or empty.** Conventional Commits format mandatory.
- ❌ **`--no-verify` git pushes.** Pre-commit hooks exist for a reason; if a hook is wrong, fix the hook in a separate PR.
- ❌ **Squashing a PR's spec change away from its implementation change.** They merge as one squash commit.
- ❌ **Releasing without all four artifacts** (Rust binary, npm package, PyPI wheel, CLI binary).
- ❌ **Manually publishing to a registry without going through the release workflow.**
- ❌ **Backdating commits or rewriting authorship.**
- ❌ **AI-style filler** in docs, comments, commit messages, or PR descriptions.
- ❌ **Time estimates** in docs or commits ("takes ~5 minutes").

### Conformance Test Anchors (meta — testing the workflow itself)

- `meta/conventional-commits/` — git log on `main` parses as Conventional Commits; non-conformant messages fail CI on PRs.
- `meta/spec-implementation-coupling/` — PRs that change `*.rs`/`*.ts`/`*.py` in a routing-critical path AND don't change `_bmad-output/project-context.md` get a CI warning labeling them for spec review.
- `meta/release-artifact-completeness/` — release tags produce all four artifacts; missing artifact fails the release pipeline.
- `meta/no-direct-main-push/` — branch protection enforced; bypass attempts logged and reverted.
- `meta/inclusive-terminology/` — regex CI gate over source files for forbidden terms (whitelist, blacklist, master, slave); allowlisted occurrences require explicit comment.
- `meta/no-ai-filler/` — regex CI gate over PR descriptions and commit messages for known AI-filler phrases.

---

## Usage Guidelines

**For AI agents implementing code in this project:**

- Read this file before writing any code touching `zorn-meshd`, the SDKs, the CLI, the dashboard, or the test harness.
- When a rule says MUST or MUST NOT, treat it as a release-gating constraint, not a suggestion.
- When in doubt between two options, prefer the more restrictive one. The architecture intentionally trades flexibility for invariant-preservation.
- The 20 Don't-Miss rules in Cat 8 are the highest-velocity reference — scan them first.
- If a rule appears wrong or impossible to follow, file a question in the PR rather than working around it.
- Spec changes ship in the same PR as the implementation that motivates them.

**For humans maintaining this file:**

- Treat the spec as the source of truth, not the implementation. When they diverge, the spec is right by definition until intentionally amended.
- Update this file whenever a Cat 1–7 rule changes. The conformance fixtures referenced here MUST exist; an orphaned anchor is a documentation bug.
- Each forbidden pattern (❌) and each MUST/MUST NOT must have a named enforcement mechanism (Cat 3 forbidden-pattern matrix, Cat 7 conformance anchors). Rules without a mechanism are decoration and should be removed or wired up.
- Keep the Don't-Miss list (Cat 8) at ≤20 items. If it grows, the items aren't all load-bearing — promote some to category-internal rules.
- Review this file at every major version bump and after any production incident to absorb new failure modes.
