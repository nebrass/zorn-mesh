---
title: "Product Brief Distillate: Zorn Mesh"
type: llm-distillate
source: "product-brief-zorn-mesh.md"
project_name: "zorn-mesh"
created: "2026-04-26"
purpose: "Token-efficient context for downstream PRD creation, story authoring, and agent prompts. Every detail captured here was either compressed or omitted from the 1-2 page executive brief because brevity demanded it. This file does NOT duplicate the technical spec at `_bmad-output/project-context.md` — it bridges the brief to the spec by capturing audience, success metrics, rejected directions, scope signals, and open questions the spec itself does not address."
related_artifacts:
  - "_bmad-output/project-context.md  # canonical technical specification"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh.md  # the executive brief"
  - "Claude.pdf  # canonical blueprint that the spec is derived from"
---

# Product Brief Distillate: Zorn Mesh

## Identity & one-line positioning

- Single-binary local message broker + protocol adapter for AI coding agents on a developer's machine.
- Mental model: "NATS-on-a-laptop with an MCP front door."
- Architectural shape: privileged-by-uid broker daemon owning socket + SQLite + registry; everything else is a client linking the SDK and connecting over UDS. Same shape as `tailscaled`, `podman system service`, `sccache --start-server`, `dockerd`.
- One sentence summary that holds the whole answer (verbatim from canonical blueprint): "Build it in Rust on Tokio, with a JSON-RPC 2.0 wire that is a strict superset of MCP, one SQLite file for everything, Unix-domain sockets gated by SO_PEERCRED, and an async-lsp-style tower service at the core."

## Audience signals (from brief + spec)

- **Primary user persona:** developer running 3–5 agents in parallel on one machine today (Cursor user wiring custom MCP servers; Claude Desktop user orchestrating Python+TS agents; Open Code user with Gemini+Copilot side-by-side; Rust developer prototyping an agent runtime).
- **Buyer = User** — developer tool, no separate procurement track.
- **Persona pain signature:** ad-hoc HTTP servers on random ports, JSON files in `/tmp`, env vars passing handles, manual prompt-engineered orchestration, no audit trail at 2 a.m. when a 3-agent pipeline breaks.
- **Secondary persona (post-MVP):** platform engineer at mid-size org standing up internal agent platform; needs local-first prototype that mirrors eventual multi-host deployment shape.
- **Explicitly NOT the audience:** teams already running NATS/Kafka for cross-machine messaging (Zorn Mesh refuses scope creep into general service mesh).
- **Adoption wedge:** the MCP-stdio bridge (`zornmesh stdio --as-agent <id>`) — any existing MCP client (Claude Desktop, Cursor, VS Code Copilot) joins the bus without modification.

## Architecture decisions (locked, do not relitigate)

- **Library + auto-spawned daemon** (`sccache` pattern). `Mesh.connect()` auto-spawns if socket absent. `ZORN_NO_AUTOSPAWN=1` opts out for production.
- **Single binary `zornmesh`** with subcommands; the daemon is `zornmesh daemon`.
- **One TypeScript SDK at parity with Rust SDK on day one** (`@zornmesh/sdk`).
- **One Envelope, one MessageType enum** (REGISTER / HEARTBEAT / REQUEST / RESPONSE / EVENT / STREAM_CHUNK / ACK / NACK / ERROR / CANCEL / SHUTDOWN). No two-tier control_frame partition.
- **Protobuf is the canonical schema** (`proto/zorn/mesh/v0/`); JSON is the canonical wire. Buf canonical-JSON mapping makes them interchangeable.
- **Wire framing is LSP-style `Content-Length:`** over UDS. NOT a 4-byte length prefix.
- **gRPC fallback transport over the same socket** for high-throughput streams (`tonic` 0.12); daemon multiplexes by sniffing first byte (`{` = JSON-RPC, `PRI` = HTTP/2 preface).
- **NATS-style hierarchical subjects** with `*` (single segment) and `>` (trailing) wildcards on subscribe only.
- **Direct messaging is `agent.<id>.inbox`** (degenerate topic).

## Wire & protocol contract (load-bearing details from canonical blueprint)

- MCP version: **2025-11-25** (handshake byte-compatible).
- A2A version: **v0.3** (Linux Foundation, June 2025; ACP merged in as of 2025-09-01).
- AGNTCY/SLIM: gRPC-heavy, MLS quantum-safe — v1.0 bridge only if demand emerges.
- ANP: research curiosity, not production.
- Zorn-specific methods live under reserved `mesh/*` namespace, never collide with MCP's `tools/*` `resources/*` `prompts/*` `sampling/*` `roots/*` `elicitation/*`.
- Symmetric capability model (unlike MCP's hub-and-spoke): both ends advertise `consumer` AND `provider` sets at handshake.
- Three-layer versioning: dated `protocolVersion` at handshake, SemVer `schema_version` per envelope, capability sets as the actual evolution mechanism.
- Always include `_UNSPECIFIED = 0` in every protobuf enum so v0 readers parse v1 envelopes without crashing on unknown values.
- `buf breaking` runs in CI against `main` to forbid wire-incompatible field reuse.

## Persistence rules (one SQLite file, period)

- Path: `~/.local/state/zornmesh/mesh.db`.
- Six tables: `agents`, `messages` (append-only log), `subscriptions`, `leases`, `dlq`, `audit`.
- PRAGMAs: `journal_mode=WAL`, `synchronous=NORMAL`, `wal_autocheckpoint=1000`, `temp_store=MEMORY`, `mmap_size=268435456`, `busy_timeout=5000`.
- Single writer connection through `deadpool-sqlite` + small read pool.
- Retention: hourly age-based DELETE (default 24h messages, 7d dlq, 30d audit).
- Compaction: `VACUUM INTO` weekly at 03:00 local.
- `fjall` enters as v0.5 option ONLY if measured write-stalls; SQLite stays for registry/leases/audit, fjall takes hot append-only log if split. Do not split prematurely.
- Two persistence stores from day one is one of the listed "biggest mistakes."

## Reliability semantics (honest claims only)

- **At-least-once delivery** with mandatory idempotency keys. Pull-based, not push.
- **Exactly-once is rejected** as architecture: physically impossible (Jepsen 2026 found NATS JetStream loses acknowledged writes under default fsync), advertising it erodes trust.
- **Lease primitive:** `mesh.fetch(stream, max=N, lease_ms=30000)` → consumer ACKs by offset before lease expiry → reaper task scans every 1s for `lease_expires_at < now AND NOT acked` → re-queues with `delivery_count++` → DLQ when `delivery_count > max_redeliveries` (default 8).
- **Idempotency:** producer-supplied optional `dedup_id` field, daemon refuses duplicates within 5-minute window via partial unique index `(stream, dedup_id) WHERE dedup_id IS NOT NULL`.
- **Heartbeats every 5s; timeout 15s.** On timeout, presence subsystem publishes `agent.<id>.down`, releases all leased messages, re-queues immediately.
- **Backoff:** `min(cap, base * 2^attempt) * rand(0,1)` — full jitter, AWS-style, NOT equal-jitter. `base=100ms`, `cap=5s`.
- **Backpressure:** bounded `tokio::sync::mpsc` channels everywhere; slow subscribers dropped with `mesh.subscriber.lagged` event rather than blocking publishers (NATS philosophy).
- **NOT in MVP and NOT in v0.5:** replication, Raft (single-machine), exactly-once, distributed transactions, total ordering across topics. These are a different product.

## Security phasing (incremental hardening)

- **v0.1 MVP:** uid-match + socket ACL only (`UnixStream::peer_cred()` on accept rejects mismatched uid; socket `chmod 0600` in directory `chmod 0700`). Abstract Unix sockets on Linux explicitly rejected (no filesystem ACL).
- **v0.2:** Ed25519 keypair per agent at `~/.config/zornmesh/keys/<agent_id>.ed25519` (mode 0600). `Signature` field already reserved (Protobuf field 14). Sensitive operations require signed envelope.
- **v0.5:** capability tokens via `biscuit-auth` 5 with macaroons-style attenuation. Daemon validates at every operation; audit log records token serial. Enables delegation (agent runtime mints short-lived sub-tokens for spawned tools).
- **macOS/FreeBSD detail:** `peer_cred()` returns euid only (no PID); use `LOCAL_PEERPID` via raw `getsockopt` for logging. Do NOT rely on PID for auth (PID reuse).
- **Out of scope explicitly:** end-to-end encryption between agents (kernel protects loopback IPC), TLS on Unix socket (pointless), DID/Verifiable Credentials (ANP-style, premature), MLS (AGNTCY's beat).
- **Daemon never runs as root.** Setuid invocation refused.
- **Prompt-injection defense is the agent's job;** Zorn Mesh provides tamper-evident, signed, audited delivery so an agent can verify provenance before trusting payload content.

## Observability conventions

- **OTel-first;** `gen_ai.*` semantic conventions for any LLM-touching span (still experimental in v1.37 SemConv as of 2026-04 but the de-facto target).
- Span name format: `mesh.<message_type> <destination>` (INTERNAL kind).
- W3C Trace Context propagates **inline in the envelope** (`trace_id`, `span_id`, `traceparent`, `tracestate`) because IPC has no headers.
- Fan-out (one publish, N subscribers): use **span links**, not parent/child.
- Required metrics (Prometheus snake_case `mesh_*` prefix): `mesh_messages_total{type,destination_kind,result}`, `mesh_bytes_total`, `mesh_queue_depth{topic}`, `mesh_agents_registered`, `mesh_request_duration_seconds{source_agent,dest_agent,type}`, `mesh_dropped_total`, `mesh_dead_lettered_total`, `mesh_nack_total`, `mesh_heartbeat_lag_seconds{agent_id}`.
- Logging: `tracing` JSON output with `opentelemetry-appender-tracing` so every log record carries active `trace_id` and `span_id`.
- Operator wiring: `OTEL_EXPORTER_OTLP_ENDPOINT` env var to Tempo/Jaeger/Honeycomb/SigNoz.
- **Cardinality discipline:** `id`, `correlation_id`, `trace_id`, `dedup_id` are span attributes, NEVER metric labels.
- **Killer feature for adoption:** `zornmesh trace <correlation_id>` — terminal command that reconstructs the full envelope timeline for a single conversation across agents, Jaeger-style without leaving the shell. UI is sugar; traceability is oxygen.

## Roadmap & deferral list

| Release | When | Headline | Phase-specific notes |
|---|---|---|---|
| v0.1 MVP | ~10 weeks from start | Linux + macOS, Rust + TS SDKs, full feature set | "Cargo install zornmesh" + "npm i @zornmesh/sdk" launch |
| v0.2 | ~6 weeks after v0.1 | Ed25519 keys, Windows named pipes, Python SDK, MCP-stdio bridge polish | Replay protection via per-envelope `id` + recent-ID set (already required for idempotency) |
| v0.5 | TBD | Capability tokens (biscuit-auth), web dashboard at `localhost:9876`, A2A gateway, optional fjall hot-log split if measured | Web dashboard is single Axum-served page reading from SQLite — no separate frontend toolchain |
| v1.0 | TBD | Stable wire protocol with deprecation policy, AGNTCY/SLIM bridge if demand exists, federation between Zorn Mesh instances on different machines via TLS-tunneled gRPC | Federation is the upgrade path for the platform-engineer secondary persona |

## v0.1 explicit NON-goals (every one of these is a deliberate "not yet")

- Per-agent cryptographic identity
- Signed envelopes
- Capability tokens
- Multi-host federation
- A2A bridge
- AGNTCY bridge
- Web dashboard
- Dynamic policy
- Any form of replication
- Windows support
- Python SDK

## Rejected ideas (with rationale, do NOT re-propose)

- **Embedding NATS server in Rust process** — pulls in Go runtime + cgo boundary + sidecar lifecycle, not "boring." Stack Overflow has zero answers when it breaks.
- **Vendor-and-embed library with no daemon** — works for in-process agent graphs only; falls apart at the OS-process boundary because pub/sub across processes needs a rendezvous point that lives outside any single process.
- **Two-tier `frame_type` discriminator (control_frame vs envelope_frame)** — Dr. Quinn's TRIZ analysis surfaced a validation-of-failed-validation deadlock that this would have addressed, but Claude.pdf rejects it: every message is one Envelope with a MessageType enum. Bootstrap-deadlock concern is left as an open question (see below).
- **Bun runtime for TS SDK** — earlier directive was Bun-only, reversed by Claude.pdf in favor of Node + pnpm + tsup + Vitest. Rationale: ecosystem stability + `@bufbuild/protobuf` is the half-size codegen.
- **TypeBox at the boundary** — replaced by `@bufbuild/protobuf` for codegen + Zod 4 at boundary.
- **`sled`** — unmaintained.
- **Abstract Unix sockets on Linux** — no filesystem ACL.
- **gRPC as the public/agent-facing transport** — JSON-RPC is canonical; gRPC is fallback only, multiplexed on the same socket.
- **Push-based delivery without bounded buffers** — OOM in production; pull + leases is the only option.
- **redb + fjall split from day one** — doubles backup story and debug surface; SQLite wins on operability over throughput at this scale.
- **Web UI before `zornmesh trace` works** — UI is sugar, traceability is oxygen.
- **`ts-proto` / `google-protobuf`** — twice the bundle size of `@bufbuild/protobuf`.
- **`grpcio` for Python** — Pydantic v2 + protobuf runtime gives better DX.
- **End-to-end encryption between agents** — kernel protects loopback IPC.
- **TLS on Unix socket** — pointless.
- **DID/Verifiable Credentials** — ANP-style, premature.
- **MLS quantum-safe** — AGNTCY's beat, not Zorn's.
- **NATS, Redis, Kafka, RabbitMQ as deployment dependencies** — operational dependencies the team would have to install/manage.

## Success metrics (north stars + tier 2)

- **North Star (v0.1):** time-to-first-coordinated-message — wall-clock seconds from `cargo install zornmesh` to two agents exchanging envelopes through the bus. Target: < 600 seconds.
- **MVP correctness gates** (cannot ship without all):
  1. Every message type round-trips through codec property tests.
  2. Lease reaper survives 10,000 simulated agent crashes in `turmoil` without losing or duplicating an envelope outside the at-least-once contract.
  3. SQLite schema migrates cleanly from empty under `cargo test --features migration-stress`.
  4. `zornmesh trace <id>` produces a complete timeline for the 3-agent example.
- **v0.5 success criteria:** 1,000 weekly active developers, ≥1 major agent runtime ships a Zorn Mesh adapter in default config, ≥1 production A2A bridge deployment outside core team.
- **v1.0 success criteria:** MCP-superset wire recognized as deployable profile in ≥1 independent agent framework; multi-host federation demonstrated.
- **Anti-metrics** (do NOT chase): peak msg/s throughput numbers (correctness first; performance tests come later), GitHub stars, npm download counts.

## Repository structure (Cargo workspace monorepo)

- `crates/zornmesh-{proto,core,store,broker,rpc,daemon,cli,sdk}` — eight crates with strict layering.
- `proto/zorn/mesh/v0/{envelope,handshake,service}.proto` — canonical schema source.
- `sdks/{typescript,python}` — non-Rust SDKs.
- `examples/three_agents.rs` — canonical integration fixture; soul of the integration test suite.
- `xtask/` — `cargo xtask gen-proto`, `cargo xtask release`, `cargo xtask fmt-all`.
- `release-plz` drives SemVer release PRs from Conventional Commits.
- Tooling: `cargo-nextest`, `cargo-deny`, `cargo-hakari`, `buf`, `pnpm` + `tsup` (TS), `uv` (Python).
- Versioning: protocol versions are dated (e.g. `2026-04-26`); crates and SDKs are SemVer; daemon binary version equals workspace version.

## Testing strategy specifics (chaos before performance)

- **Unit:** `cargo nextest`, sub-second feedback. Codec, capability negotiation, subject/topic matcher (NATS-style trie), backoff calculator, dedup window.
- **Property** (mandatory `proptest = "1"` with shrinking on these specifically): codec round-trip JSON↔Protobuf↔Rust types preserves equality; lease state machine (no envelope is both leased AND re-queued; total in-flight count bounded); dedup window (no false positives outside, no false negatives inside).
- **Integration:** spawn daemon in-process, real `tokio::net::UnixStream` clients. Catches framing, ACL, and capability-negotiation bugs unit tests miss.
- **Chaos:** `turmoil` for deterministic clock skew, agent crashes mid-stream, slow consumers, sockets blocking on accept (where lease-reaper bugs surface). `loom` for routing-table concurrency invariants.
- **Contract:** recorded JSON fixtures one per message type, golden-tested. `buf breaking` in CI for wire stability.
- **Python cancellation suite ships with Python SDK in v0.2** and runs every PR — highest 2am-page risk.

## Top 8 "biggest mistakes to avoid" (from canonical blueprint)

1. Embedding NATS in MVP (Go runtime, premature).
2. Two persistence stores from day one (doubles operational surface).
3. Defining a Zorn-native wire incompatible with MCP (kills "MCP superset" claim).
4. Implementing exactly-once semantics (impossible, lying erodes trust).
5. Adding web UI before `zornmesh trace` works (UI is sugar, traceability is oxygen).
6. Using `sled` (unmaintained).
7. Abstract Unix sockets (no ACL).
8. Push-based delivery without bounded buffers (OOM in production).

## Open questions (surfaced but not resolved)

- **Bootstrap-deadlock concern raised by Dr. Quinn's earlier review** (the validation-of-failed-validation-ack problem) is structurally unaddressed by the single-Envelope design. May surface as a real bug under certain partial-handshake failure modes. Worth a property-test exploration before v0.1 lock.
- **Demand-signal threshold for AGNTCY/SLIM bridge** — Claude.pdf says "v1.0 if demand exists." What's the demand metric and threshold? (suggestion: 3 production deployments asking for it, or one paying customer.)
- **Federation transport choice for v1.0** — TLS-tunneled gRPC is named, but the wire protocol stability contract under federation is unspecified. Likely needs a separate design doc when the time comes.
- **Web dashboard scope at v0.5** — Claude.pdf says "single Axum-served page, no separate frontend toolchain." What's the minimum feature set: live agent topology, message timeline, DLQ inspection, replay tool? Needs a dashboard PRD as a separate exercise.
- **Capability-token distribution / bootstrapping at v0.5** — biscuit-auth root key generation, rotation policy, operator UX for issuing first token to a new agent.
- **Windows named-pipe path + ACL story for v0.2** — `\\.\pipe\zornmesh-<sid>` is the path, but per-pipe SDDL/DACL strategy and the SID equivalent of `peer_cred()` are not yet specified.
- **TypeScript SDK runtime reversal back-compat** — earlier in the spec lifecycle a Bun-only TS SDK was promised; reversed to Node + pnpm. If any pre-v0.1 alpha consumers built against Bun, communicate the reversal explicitly.
- **Python SDK timing** — Claude.pdf phases it to v0.2. Earlier directive (since reversed) was "all features v1." Confirm with stakeholders that Python users can wait ~16 weeks total for SDK availability.
- **Dependency on tonic/hyper for gRPC fallback** — pulled in to support the multiplexed first-byte-sniff pattern. Worth confirming the binary-size and compile-time cost is acceptable on a "single binary, ~22 MB" target.

## Hidden requirements implicit in the spec (distilled for PRD)

- Daemon must reach "accepting connections" within 200 ms cold start (auto-spawn invariant).
- Single envelope payload limit: ≤ 1 MiB (rejected at ingress).
- Idempotency key length: ≤ 128 bytes (validated by byte length, not character count).
- Idempotency window: 5 minutes default, configurable per capability up to 300 s — wait, blueprint says 5 min default.
- Stream chunk byte cap example given: 65536 bytes per chunk.
- Heartbeat cadence: every 5 s; agent down threshold: 15 s.
- Capability-token TTL: ≤ 5 minutes (when v0.5 lands).
- Daemon memory footprint: 5–15 MB RSS target (vs Go's 30–80 MB on developer laptops).
- Local OTel collector is optional; daemon must be fully usable without one (Prometheus scrape endpoint on loopback gateway is required, not optional).

## Pointers to deep spec (when downstream workflows need depth)

- **Full prescriptive spec:** `_bmad-output/project-context.md` — 8 categories: technology stack, protocol/envelope discipline, language-specific rules, persistence/reliability, security, observability, testing, workflow.
- **Canonical blueprint source:** `Claude.pdf` (in repo root) — 22 pages, the authoritative document. When this distillate or the spec disagrees with Claude.pdf, Claude.pdf wins.
- **Background research context:** `GeminiReport.pdf` (industry survey, 14 pages) and `Perplexity.pdf` (architectural patterns, 27 pages).
- **Repository implementation plan:** Claude.pdf §14 has a "Day 1 — schema and codec, Day 2 — broker core, ..." five-day implementation plan that should drive the first sprint's stories.

## Voice & tone (carry-forward for downstream comms)

- Crisp, precise prose. Every token earns its place.
- No AI filler, no time estimates, no unjustified emojis.
- Direct verdict over hedging.
- Boring tech preferred; novelty defended only when it earns its keep.
- Inclusive terminology (allowlist/blocklist, primary/replica).
- The user (Nebrass) is a senior engineer, intermediate skill level per BMad config, and the audience is peer engineers — write to that level.
