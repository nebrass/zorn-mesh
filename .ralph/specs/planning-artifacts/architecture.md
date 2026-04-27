---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8]
inputDocuments:
  - "_bmad-output/planning-artifacts/prd.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh.md"
  - "_bmad-output/planning-artifacts/product-brief-zorn-mesh-distillate.md"
  - "_bmad-output/planning-artifacts/prd-validation-report.md"
  - "_bmad-output/project-context.md"
workflowType: 'architecture'
project_name: 'zorn-mesh'
user_name: 'Nebrass'
date: '2026-04-27'
projectStatus: 'greenfield'
projectType: 'developer_tool'
domain: 'developer_infra'
complexity: 'high'
lastStep: 8
status: 'complete'
completedAt: '2026-04-27'
---

# Architecture Decision Document

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

## Project Context Analysis

### Requirements Overview

**Functional Requirements:**

The PRD defines **62 functional requirements** across eleven capability areas:

1. **Wire & Messaging (FR1-FR10):** publish/subscribe, request/reply, pull fetch with leases, streaming, ACK/NACK, cancellation, idempotency, durable subscriptions, and backpressure. Architecturally, this requires a protocol core, routing table, subscription matcher, delivery state machine, lease manager, idempotency gate, and explicit flow-control path.
2. **Identity & Capabilities (FR11-FR14):** agent-card registration, symmetric capability advertisement, high-privilege capability marking, and registry inspection. This requires an agent registry, capability registry, schema/version model, trust policy input, and stable identity lifecycle.
3. **Daemon Lifecycle (FR15-FR21):** SDK auto-spawn, explicit opt-out, CLI-managed daemon operations, single-daemon ownership, privilege refusal, graceful drain, and `doctor`. This requires robust rendezvous, PID/lock/socket ownership, startup race handling, signal handling, shutdown orchestration, and diagnostics.
4. **Persistence & Forensics (FR22-FR28):** durable audit log, DLQ, trace reconstruction, replay, structured inspection, retention, and offline audit verification. This requires a SQLite persistence boundary, transaction discipline, replay-safe retention, audit hash chain, and query surfaces for CLI and local UI consumers.
5. **Observability & Tracing (FR29-FR32):** W3C tracecontext propagation, OTel metrics/traces, live tail, and span-tree reconstruction. This makes observability part of the architecture, not instrumentation afterthought.
6. **Host Integration (FR33-FR35):** MCP-stdio bridge, canonical identity across host connections, and graceful degradation to baseline MCP. This requires protocol adaptation without violating the core mesh envelope semantics.
7. **Security & Trust (FR36-FR40):** secret redaction, signature/SBOM verification, high-privilege gate enforcement, and local socket permission rejection. This requires a default-deny security layer, secret-safe diagnostics, and release-integrity architecture.
8. **Compliance & Audit (FR41-FR44):** EU AI Act traceability, evidence export, personal-data deletion/redaction, and NIST AI RMF mapping. This requires data lineage, audit export bundles, redaction semantics that preserve audit integrity, and explicit compliance evidence artifacts.
9. **Developer & Operator CLI (FR45-FR48):** JSON output, non-interactive mode, completions, and stable exit codes. CLI behavior must be architected as a contract, not ad-hoc formatting.
10. **Local Web Companion UI (FR49-FR60):** `zornmesh ui`, live connected-agent roster, daemon-sequenced timeline, trace detail, Focus Trace Reader, safe direct send, safe broadcast, per-recipient outcomes, reconnect/backfill, local trust state, CLI handoff commands, and auditable human-originated UI sends. This requires a daemon-owned UI API/live transport, explicit session/security model, bundled assets, browser fixtures, and shared UI/API state taxonomies.
11. **Adopter Extensibility (FR61-FR62):** Rust/TypeScript SDK parity at v0.1 and per-call idempotency/trace/timeout propagation. This requires shared conformance fixtures and SDK API discipline.

**Non-Functional Requirements:**

The PRD defines **63 NFRs**. The strongest architectural drivers are:

- **Performance:** 200 ms p95 cold start, 5,000 envelopes/sec sustained throughput, p50 <= 2 ms / p99 <= 20 ms request-reply, 50 MiB/sec streaming, <= 256 MiB RSS under nominal load, and backpressure surfaced within 100 ms.
- **Security:** UDS-only at v0.1, socket mode `0600`, refuse root/admin execution, strict secret redaction, signed release artifacts and SBOMs, default-deny high-privilege capability gating, schema validation before processing, and no outbound network except disabled-by-default OTel export.
- **Reliability:** single-daemon invariant under concurrent connect, replay after SIGKILL, WAL recovery <= 2 s, graceful shutdown with 10 s default / 60 s max budget, disk-full read-degraded mode, and atomic forward-only migrations.
- **Scalability:** single-machine scope, ~200 connected-agent non-goal boundary, 4,096 total subscription cap, 8 subject levels / 256-byte subjects, 8 MiB envelope cap, 256 KiB stream chunks, table-specific retention, and metrics cardinality cap.
- **Compatibility:** Linux/macOS v0.1 matrix, Rust + TypeScript SDKs at v0.1, Python v0.2, MCP host conformance for Claude Desktop/Cursor, wire stability via breaking-change gates, and `NO_COLOR`/TTY behavior.
- **Observability:** tracecontext propagation, documented OTel schema, tracing overhead budget, and `doctor` latency constraints.
- **Maintainability:** reproducible builds, coverage gates, conformance fixture coverage, deterministic testing, public API docs enforcement, and per-SDK build-version independence.
- **Compliance/Auditability:** audit hash chain, retention purge evidence, evidence bundle export, GDPR-style redaction while preserving correlation/audit integrity, and complete SBOM coverage.
- **Local UI:** explicit `zornmesh ui` launch, loopback-only UI/API listener, per-launch session token, CSRF/origin checks, bundled assets only, browser support, reconnect/backfill, browser E2E fixture coverage, and WCAG AA accessibility.

**Scale & Complexity:**

- Primary domain: **developer infrastructure / local IPC / AI-agent protocol broker**.
- Complexity level: **high**. The product is local-first, but it combines protocol design, daemon lifecycle, cross-language SDKs, persistence, forensic replay, security boundaries, compliance evidence, and conformance testing.
- Estimated architectural components: **16**.
  1. SDK auto-spawn/rendezvous layer
  2. Daemon process lifecycle and ownership layer
  3. Wire framing and protocol validation layer
  4. Agent identity and capability registry
  5. Subject matcher and routing core
  6. Delivery semantics engine: leases, ACK/NACK, retry, idempotency, DLQ
  7. Streaming and backpressure subsystem
  8. SQLite persistence and migration subsystem
  9. Audit-log and evidence-export subsystem
  10. Observability subsystem: traces, metrics, logs, Prometheus/OTLP surfaces
  11. Security/policy subsystem: socket trust, high-privilege gate, redaction, release integrity
  12. CLI/operator surface
  13. MCP-stdio bridge and future adapter boundaries
  14. Local UI API and live-event gateway: explicit `zornmesh ui` loopback listener, session/token protection, CSRF/origin enforcement, REST/read APIs, state-changing send APIs, WebSocket/SSE live channel, reconnect/backfill, and audit hooks.
  15. Local web app and bundled asset pipeline: Bun-managed React/Next.js app, Tailwind/Radix-style accessible primitives, static asset packaging, no-external-request enforcement, browser E2E fixtures, responsive fixtures, and accessibility checks.
  16. Test harness, conformance fixtures, chaos/property/performance test infrastructure

### Source Authority & Conflict Resolution

Architecture must prevent implementation agents from inheriting contradictions across the loaded documents. The authority hierarchy for this workflow is:

1. **PRD controls product scope and user-facing requirements.** FRs, NFRs, journeys, launch gates, and non-goals define what must be true.
2. **Project context controls technical implementation constraints.** When the PRD or brief names a technology/mechanism that conflicts with `_bmad-output/project-context.md`, the architecture should treat project-context as the current technical authority unless the contradiction is escalated as an explicit architecture decision.
3. **Product brief and distillate are historical context.** They explain intent and market positioning, but they contain older technical decisions and must not override the finalized PRD or project-context.
4. **Architecture resolves contradictions for implementation.** Any unresolved conflict becomes an implementation blocker and must be decided before epics/stories are generated.
5. **Current PRD and UX supersede earlier no-GUI architecture text for v0.1 local UI scope.** Any architecture section that says no GUI/frontend/static assets ship in v0.1 is superseded by current PRD FR49-FR60, UI NFRs, UX specification, and this amendment.

### PRD ↔ Project-Context Conflict Matrix

| Conflict Zone | PRD / Brief Signal | Project-Context Signal | Architecture Handling |
|---|---|---|---|
| Wire framing | PRD/brief mention LSP-style `Content-Length` framing. | Project-context mandates `[length:u32 BE][frame_type:u8][payload]` and frame-type discrimination. | Treat as open architecture decision in Step 4; default bias to project-context because it resolves control/envelope validation deadlock and has conformance anchors. |
| Control frames vs envelope-only ACK/NACK | PRD FRs mention ACK/NACK as broker capabilities; brief/distillate contain older single-envelope model. | Project-context mandates separate control-frame tier for ACK/NACK/STREAM_END/PING/PONG/CAPABILITY_PROBE. | Architecture must decide protocol tiering explicitly; implementation cannot proceed with both models. |
| gRPC scope | PRD includes gRPC fallback transport in v0.1 scope in places. | Project-context permits gRPC only for OTLP export; public/agent-facing gRPC is forbidden. | Escalate in architecture decisions. Until resolved, do not put gRPC data-plane stories in v0.1. |
| TypeScript runtime / package manager | PRD and distillate contain Node/pnpm/tsup/Vitest references in places. | Project-context mandates Bun, `.bun-version`, Bun package manager/test runner/bundler, and no Node/npm/pnpm/yarn. | Project-context wins unless product scope is amended; architecture should specify Bun-only TypeScript SDK and local UI tooling. |
| Schema boundary | PRD emphasizes Protobuf canonical schema and JSON wire. | Project-context splits schema layers: TypeBox -> JSON Schema for capability contracts; Protobuf for canonical internal envelope/registry/audit; both meet at UDS ingress. | Architecture must separate capability schema ownership from envelope/internal model ownership. |
| SQLite driver and pragmas | Brief/distillate mention rusqlite/deadpool and older pragma/mmap/retention details. | Project-context mandates sqlx, writer/reader pools, writer `synchronous=FULL`, forward-only sqlx migrations, revised cache/mmap budgets. | Project-context wins; persistence architecture must use sqlx and per-pool pragma policy. |
| Retention defaults | Older docs mention general/default retention values. | Final PRD and project-context converge on messages 24 h, DLQ 7 d, audit log 30 d. | Resolved: table-specific retention defaults are controlling. |
| SDK language matrix | PRD says Rust + TypeScript at v0.1, Python v0.2. | Project-context defines Rust, Bun TypeScript, Python 3.11-3.13 when Python lands. | Resolved: Rust + Bun TypeScript at v0.1; Python deferred to v0.2 but architecture should reserve shared conformance interfaces. |
| MCP version references | PRD/brief mention MCP 2025-11-25 in places; project-context examples mention MCP 2025-03-26 initialize shape. | Both require MCP-compatible `initialize`; date/version must be normalized before implementation. | Open architecture/documentation correction: choose the MCP version target and update downstream artifacts consistently. |

### Component-to-Requirement Coverage

The 16-component decomposition is preliminary but traceable:

| Component | Primary FR Coverage | Primary NFR Coverage |
|---|---|---|
| SDK auto-spawn/rendezvous layer | FR15, FR16 | NFR-P1, NFR-R1, NFR-C2 |
| Daemon lifecycle and ownership | FR17-FR21 | NFR-R1, NFR-R4, NFR-R6, NFR-O4 |
| Wire framing and protocol validation | FR1-FR8, FR33-FR35 | NFR-S7, NFR-C3, NFR-C4, NFR-SC4 |
| Agent identity and capability registry | FR11-FR14, FR34 | NFR-S6, NFR-SC2 |
| Subject matcher and routing core | FR1-FR3, FR9-FR10 | NFR-P2, NFR-P3, NFR-SC2, NFR-SC3 |
| Delivery semantics engine | FR4-FR10, FR23, FR25 | NFR-R2, NFR-R5, NFR-P6 |
| Streaming and backpressure subsystem | FR5, FR7, FR10 | NFR-P4, NFR-P6, NFR-SC4 |
| SQLite persistence and migrations | FR22, FR23, FR25-FR27 | NFR-R2, NFR-R3, NFR-R5, NFR-R6, NFR-SC5 |
| Audit log and evidence export | FR28, FR41-FR44 | NFR-CA1-NFR-CA5 |
| Observability subsystem | FR29-FR32 | NFR-O1-NFR-O3, NFR-SC6 |
| Security and policy subsystem | FR13, FR36-FR40 | NFR-S1-NFR-S8 |
| CLI/operator surface | FR17, FR21, FR24-FR28, FR31, FR37-FR38, FR42, FR45-FR48 | NFR-C5, NFR-O4, NFR-CA3 |
| MCP-stdio bridge and adapters | FR33-FR35 | NFR-C4 |
| Local UI API and live-event gateway | FR49-FR60 | NFR-P7-NFR-P9, NFR-S1, NFR-S9-NFR-S12, NFR-R7, NFR-C6-NFR-C7, NFR-M7, NFR-A11Y1-NFR-A11Y6 |
| Local web app and bundled asset pipeline | FR49-FR60 | NFR-P7-NFR-P9, NFR-S9-NFR-S12, NFR-C6-NFR-C7, NFR-M7, NFR-A11Y1-NFR-A11Y6 |
| Rust/Bun TypeScript SDKs | FR61-FR62 | NFR-C2, NFR-M1-NFR-M6 |
| Test harness and conformance infrastructure | All FRs through acceptance fixtures, including browser UI fixtures | NFR-M2-NFR-M5, NFR-M7 |

### CLI/UX Ergonomics Contract

The architecture treats CLI behavior and the local web UI as complementary v0.1 user experience contracts. The CLI remains the durable automation, recovery, and offline audit surface. The local web UI is the interactive observe -> inspect chronology -> send safely -> confirm outcome surface. Both must share state names, error categories, redaction behavior, trace/correlation identifiers, and recovery handoff commands.

**Command hierarchy:**

- Primary developer workflows: `trace`, `tail`, `inspect`, `replay`, `agents`.
- Operator workflows: `daemon`, `doctor`, `audit verify`, `inspect sbom`.
- Integration workflow: `stdio --as-agent <id>`.
- Discovery/help workflow: `zornmesh --help`, `zornmesh <cmd> --help`, and generated shell completions.

**Output contracts:**

- Read commands support `--output json`; JSON output is stable and testable.
- Streaming commands emit NDJSON when `--output json` is used.
- Human output uses aligned tables or timelines, disables ANSI when stdout is not a TTY, and honors `NO_COLOR=1`.
- Error output is human-actionable: what failed, why it failed, and the next diagnostic command when available.

**Interactivity and automation:**

- `--no-input` guarantees no prompts; if interaction would be required, the command fails fast with a stable exit code.
- Long-running commands handle Ctrl+C as graceful cancellation and preserve trace/audit integrity.
- `tail` and streaming surfaces must define interruption behavior, flush behavior, and pipe-safe output.

**Configuration precedence:**

- Daemon flags override env vars; env vars override config files; config files are local only.
- CLI flags override all for the current invocation.
- Remote config fetch is forbidden.

### Technical Constraints & Dependencies

The loaded documents establish several architectural constraints:

- **Local-first invariant:** no separately managed broker dependency. No NATS, Redis, Kafka, RabbitMQ, Docker runtime requirement, or cloud service in the core.
- **Daemon shape:** library/SDK calls `connect()`, attempts UDS connect, auto-spawns daemon if absent, then retries. Users should not have to manually start a daemon for the laptop-default path.
- **Single-user trust boundary:** v0.1 trust anchor is kernel UDS peer credentials plus socket ACLs. Daemon must refuse root/elevated operation.
- **Persistence:** SQLite is the only persistence engine. The architecture must protect ACK-after-commit durability, WAL recovery, retention, DLQ, replay, and audit evidence.
- **Protocol compatibility:** MCP compatibility is a launch gate. Zorn-specific behavior must not break baseline MCP host interop.
- **Identity model:** A2A AgentCard identity and capability descriptors are the preferred model in the project context and PRD.
- **Observability:** OpenTelemetry is mandatory. `zornmesh trace <correlation_id>` is the product's killer workflow and must be architected from the start.
- **Cross-SDK conformance:** Rust and TypeScript ship at v0.1; Python follows v0.2. Shared protocol fixtures are required to prevent semantic drift.
- **Release integrity:** Sigstore signatures and CycloneDX SBOMs are release-blocking requirements, not packaging polish.
- **Documentation and compliance:** compliance mappings and evidence export are part of the product surface.

### Implementation Blockers to Resolve in Architecture Decisions

The following items must be resolved before epics/stories are generated:

1. **Wire framing:** LSP-style `Content-Length` vs 4-byte length prefix + frame type.
2. **Protocol tiering:** envelope-only ACK/NACK vs separate control-frame tier.
3. **gRPC role:** data-plane fallback vs OTLP-only export transport.
4. **MCP target version:** normalize the MCP version used for handshake/conformance.
5. **TypeScript runtime:** confirm Bun-only implementation despite PRD/brief historical Node references.
6. **Schema ownership:** separate capability JSON Schema, JSON-RPC envelope validation, and Protobuf internal model.
7. **Persistence implementation:** sqlx writer/reader pool architecture and per-pool pragmas.
8. **CLI output schemas:** define stable JSON/NDJSON shapes before CLI stories are written.

### Cross-Cutting Concerns Identified

- **Protocol/versioning discipline:** frame format, JSON-RPC/MCP compatibility, envelope schema, capability versions, error-code registry, and backward compatibility must be decided once and enforced everywhere.
- **Lifecycle/rendezvous correctness:** auto-spawn, socket path, lock/PID ownership, concurrent connect, daemon readiness, explicit mode, signal handling, and cleanup affect SDKs, CLI, tests, and docs.
- **Persistence and delivery semantics:** messages, leases, idempotency, retries, DLQ, retention, replay, audit, and crash recovery form one coupled reliability domain.
- **Security and trust:** socket permissions, peer credentials, default-deny capability policy, high-privilege allowlists, secret redaction, and release integrity must be consistent across daemon, SDKs, CLI, and docs.
- **Observability and forensics:** trace IDs, correlation IDs, span tree shape, CLI trace rendering, metrics cardinality, bootstrap logging, and OTel export must be designed as one system.
- **CLI as UX:** JSON schemas, human-readable tables, `NO_COLOR`, exit codes, prompts, non-interactive mode, and shell completions are user-facing contracts.
- **Cross-language parity:** Rust, TypeScript, and Python need shared conformance fixtures, generated schemas/bindings, equivalent error semantics, and equivalent lifecycle behavior.
- **Compliance evidence:** audit-log integrity, retention, evidence export, SBOM, signature verification, and redaction need explicit implementation paths and test anchors.
- **Greenfield implementation sequencing:** architecture must produce boundaries that support story slicing without front-loading every table/model or creating technical-only epics.

## Starter Template Evaluation

### Technical Preferences Found

The project context is prescriptive enough that no general-purpose starter should be allowed to choose core architecture defaults.

**Languages / runtimes:**

- Rust stable channel, edition 2024, MSRV policy = latest stable minus 2 minor versions and must include 1.85+.
- Tokio is the only Rust async runtime.
- TypeScript SDK and local UI tooling use Bun only: `.bun-version`, `packageManager: bun@x.y.z`, `bun install`, `bun test`, and Bun bundling/runtime where applicable.
- Python SDK is deferred to v0.2 and targets CPython 3.11, 3.12, and 3.13 with `uv`.

**Core libraries / tools:**

- Rust daemon/CLI: Tokio, clap, sqlx, OpenTelemetry, thiserror, and dedicated platform-syscall crates where needed.
- Persistence: SQLite via sqlx, WAL mode, single writer pool, read-only reader pool, forward-only migrations.
- TypeScript schema/runtime: TypeBox for capability JSON Schema, `@bufbuild/protobuf` for Protobuf bindings where needed, Bun test runner.
- Python future SDK: Pydantic v2, betterproto, mypy strict, asyncio/anyio.
- Repo tooling: Cargo workspace, `just`, `cargo xtask`, Bun, uv, conformance fixtures, lockfiles committed.

**Development patterns:**

- No external runtime dependency: no NATS, Redis, Kafka, RabbitMQ, Docker runtime requirement, or cloud SDK in core.
- Daemon auto-spawn via SDK connect path, with explicit opt-out.
- UDS/named-pipe transport boundary; no TCP listener at v0.1.
- Conformance-first development: protocol, persistence, security, observability, CLI, and meta fixtures must be created early.
- CLI is a first-class UX surface, not a debug afterthought.

### Primary Technology Domain

**Custom Rust daemon + CLI + cross-language SDK monorepo**, not a generic web/API/full-stack app.

The closest generic category is **CLI tool / system daemon**, but zorn-mesh exceeds ordinary CLI starter scope because it also needs local broker daemon lifecycle, wire protocol contracts, SQLite persistence, cross-SDK conformance, MCP-stdio host integration, release signatures/SBOMs, and terminal UX fixtures.

### Current Ecosystem Check

Current-version checks performed during this step:

- Bun public site advertises **Bun v1.3.13** as current.
- crates.io API reports:
  - `tokio` **1.52.1**
  - `clap` **4.6.1**
  - `sqlx` **0.8.6**
- Generic Rust CLI/daemon starter options found include `rust-cli/cli-template`, `rust-starter`, and cargo-generate community templates.

These starters are maintained enough to inspect for conventions, but none should be adopted wholesale because each would import defaults that conflict with project-context requirements or omit required monorepo/conformance structure.

### Starter Options Considered

| Option | Fit | Decision |
|---|---|---|
| `rust-cli/cli-template` | Good for a single Rust CLI using clap; insufficient for daemon + SDK + SQLite + protocol conformance monorepo. | Reject as base; inspect only for CLI convention ideas. |
| `rust-starter` / generic Rust production starters | Useful CI/config reference; too generic and likely to include Docker/web/service assumptions that are not product invariants. | Reject as base; inspect only for CI hygiene patterns. |
| Cargo-generate custom template | Could work if zorn-mesh becomes a repeated pattern later. Not needed for this one greenfield repo. | Defer. Do not create a public template now. |
| Manual/custom repository scaffold | Best fit. Allows architecture to encode exact crate boundaries, Bun/uv directories, conformance fixtures, migrations, release artifacts, and CLI UX fixtures without starter drift. | **Selected.** |

### Selected Starter: Custom Zorn Mesh Workspace Scaffold

**Rationale for Selection:**

A public starter would solve the wrong problem. Zorn Mesh is not merely a Rust CLI; it is a protocol broker, daemon, CLI, SDK, persistence engine, conformance corpus, and release pipeline in one monorepo. The starter must preserve product-specific constraints from day one rather than retrofitting them after generic scaffolding.

This selection keeps the first implementation story focused on creating a compileable, testable skeleton with the correct boundaries. It avoids importing starter defaults that conflict with the architecture, especially around runtime choice, persistence, Docker, web frameworks, test runners, and package managers.

### Initialization Command

No upstream starter command is selected. The first implementation story should create the repository scaffold explicitly.

Representative scaffold sequence:

```bash
mkdir -p crates sdks conformance docs migrations test-infra fixtures/cli-output .github/workflows
cargo new --lib crates/zornmesh-proto
cargo new --lib crates/zornmesh-core
cargo new --lib crates/zornmesh-store
cargo new --lib crates/zornmesh-broker
cargo new --lib crates/zornmesh-rpc
cargo new --lib crates/zornmesh-sdk
cargo new --bin crates/zornmesh-daemon
cargo new --bin crates/zornmesh-cli
cargo new --bin xtask
mkdir -p sdks/typescript
(cd sdks/typescript && bun init --yes)
```

The exact command sequence should be finalized in the first implementation story and aligned with the naming decision for `zornmesh` vs `zorn` vs `zorn-meshd`. No scaffold command should run before Step 4 resolves binary/package naming.

### Architectural Decisions Provided by Starter

**Language & Runtime:**

- Rust workspace is the root implementation structure.
- Rust edition **2024** is required by project-context and supported by the MSRV policy.
- Root virtual workspace `Cargo.toml` uses `resolver = "3"` for Rust 2024 dependency resolution behavior.
- Tokio is the only async runtime.
- TypeScript SDK lives under `sdks/typescript` and uses Bun.
- Python SDK does **not** ship at v0.1. The scaffold may include `sdks/python/README.md` as a v0.2 boundary marker, but it must not include fake package code or failing placeholder tests.

**Feature-Gate Policy:**

- Scaffold defaults are minimal and buildable.
- No feature may introduce an alternate Rust async runtime.
- No gRPC data-plane feature should be created until Step 4 resolves gRPC scope.
- Feature flags must not hide core invariants such as validation, socket trust, secret redaction, or ACK-after-commit durability.
- Optional platform integrations must be additive and tested through explicit feature combinations.

**Styling Solution:**

- v0.1 includes a local web UI launched by `zornmesh ui`.
- The UI uses a Bun-managed React/Next.js app shell with Tailwind-aligned tokens and Radix-style accessible primitives.
- Project-owned UI primitive wrappers own domain-specific surfaces: live agent roster, message/trace timeline, trace detail, delivery-state badges, local daemon status, CLI handoff blocks, safe direct composer, and safe broadcast confirmation.
- All UI assets are bundled with the release artifact. Runtime CDN scripts, remote fonts, analytics, images, and remote config are forbidden.
- CLI human output style remains governed by the CLI/UX Ergonomics Contract: aligned tables/timelines, `NO_COLOR`, TTY-aware ANSI, JSON/NDJSON contracts.

**Build Tooling:**

- Cargo workspace at repository root.
- `cargo xtask` for cross-language generation, conformance orchestration, release checks, fixture validation, and multi-tool CI commands.
- `justfile` as the top-level task runner dispatching to Cargo, Bun, and uv.
- Bun lockfile and `.bun-version` when TypeScript SDK lands.
- `uv.lock` when Python SDK work starts.
- Lockfiles committed and checked in CI.

**Testing Framework:**

- Rust: `cargo test`/`cargo nextest`, proptest, loom, turmoil, cargo-llvm-cov or equivalent coverage tooling.
- TypeScript: `bun test` only.
- Python v0.2: pytest + mypy strict + coverage.py + project-specific ruff rules.
- Shared conformance corpus under `/conformance/` from the first scaffold story.
- In-tree test harness binary is planned as a first-class workspace target, not as per-SDK ad hoc test code.
- CLI golden-output fixtures are included from the scaffold phase so terminal UX is testable early.

**Code Organization:**

Initial workspace should establish these boundaries:

- `crates/zornmesh-proto` — generated/internal protocol bindings and schema integration points.
- `crates/zornmesh-core` — domain types, envelope primitives, IDs, capability refs, errors.
- `crates/zornmesh-store` — SQLite schema, migrations, writer/reader pool, persistence actors.
- `crates/zornmesh-broker` — routing, subject matching, delivery state machine, leases, backpressure.
- `crates/zornmesh-rpc` — UDS transport, framing, JSON-RPC/MCP handshake, gateway boundaries.
- `crates/zornmesh-daemon` — daemon binary, lifecycle, signal handling, startup/shutdown orchestration.
- `crates/zornmesh-cli` — CLI binary and operator/developer commands.
- `crates/zornmesh-sdk` — Rust SDK connect/register/publish/subscribe/request APIs.
- `sdks/typescript` — Bun TypeScript SDK.
- `apps/local-ui` — Bun-managed React/Next.js local UI app, component wrappers, browser fixtures, and bundled asset pipeline.
- `sdks/python` — README-only v0.2 boundary marker, or omitted until v0.2.
- `conformance/` — shared executable fixtures consumed by daemon and SDK suites.
- `fixtures/cli-output/` — golden CLI output examples and fixtures.
- `migrations/` — forward-only sqlx migrations.
- `test-infra/` — OTel collector and integration harness assets.
- `docs/` — operator, compliance, adapter-author, and reference docs.

### Crate Dependency DAG

Initial dependency direction should be acyclic and enforce layering:

```text
zornmesh-proto
  ↓
zornmesh-core
  ↓
├─ zornmesh-store
├─ zornmesh-rpc
└─ zornmesh-sdk
      ↓
zornmesh-broker depends on: core, store, rpc
zornmesh-daemon depends on: core, store, broker, rpc
zornmesh-cli depends on: core, sdk, rpc (for local client operations)
xtask depends on: no product crates unless needed for codegen/fixture validation
```

Rules:

- Lower crates must not depend on higher crates.
- SDK must not depend on daemon or broker internals.
- CLI may use SDK for client-style operations but must not open SQLite directly.
- Store must not depend on broker routing logic.
- RPC/framing must not depend on CLI or daemon process lifecycle.
- Platform-syscall wrappers, if needed, should live in narrow platform crates or modules with reviewed unsafe boundaries.

### Conformance Scope

`conformance/` is an executable specification corpus, not miscellaneous test data.

- **Fixtures:** stable JSON/TOML/protobuf/vector files for protocol, idempotency, handshake, AgentCard, timestamp, streaming, persistence, security, observability, CLI output, and meta rules.
- **Consumers:** Rust daemon/integration tests, Rust SDK, TypeScript SDK, future Python SDK, and `xtask` validation commands.
- **Direction:** fixtures do not depend on any implementation. Implementations consume fixtures.
- **Runtime harness assets:** OTel collector config, debug/fault injection assets, and process orchestration live in `test-infra/`, not mixed into fixture files.
- **Python boundary:** no Python API contract exists at v0.1. Fixtures must remain language-neutral so Python v0.2 can consume them without rewriting protocol evidence.
- **CLI golden outputs:** root help, command help, trace success JSON, trace not-found human error, stderr examples, and NDJSON tail examples should be covered either under `conformance/cli-output/` or `fixtures/cli-output/`, then wired into tests.

### Build Contract

The scaffold must define how each ecosystem participates in build/test without ambiguity:

- `just` is the human entrypoint: `just fmt`, `just test`, `just conformance`, `just lint`, `just build`, `just docs`.
- `cargo xtask` is the orchestrator for generated artifacts, fixture checks, release preflight, and cross-language commands.
- Rust CI runs workspace check/test/lint/doc/coverage gates.
- TypeScript CI runs `bun install --frozen-lockfile` once `bun.lockb` exists, then `bun test`.
- Python CI is not required for v0.1 unless a real Python package exists; if only a README boundary exists, Python tests are not wired yet.
- Lockfile drift is a CI failure once lockfiles exist.
- CI must reject unsupported runtime/tooling drift, especially Node/npm/pnpm/yarn and alternate Rust async runtimes.

### CLI Golden-Output Fixtures

The starter should create a fixture home for terminal UX contracts before implementation diverges.

Minimum early fixtures:

- `help-root.txt` — expected `zornmesh --help` or final binary-name equivalent; must surface `trace` as the hero forensic workflow.
- `help-trace.txt` — expected trace command help with examples.
- `trace-success.json` — stable JSON shape for a reconstructed trace.
- `trace-not-found.txt` — human error for unknown correlation ID, including a recovery hint such as `tail`, `inspect`, or `doctor` once those command semantics are finalized.
- `trace-not-found.stderr.txt` — stderr wording and stable exit-code expectation.
- `tail-example.ndjson` — pipe-safe streaming example.
- `doctor-healthy.json` — stable machine output for healthy daemon diagnostics.

Output semantics:

- Read commands use `--output human|json`.
- Streaming JSON output is NDJSON.
- Human output goes to stdout on success.
- Errors go to stderr and use stable wording plus stable exit codes.
- TTY color is disabled for non-TTY output and whenever `NO_COLOR=1` is set.

These fixtures can start as design fixtures and become test fixtures as commands are implemented.

### First Implementation Story Guidance

Project initialization should be the first implementation story, but it must not be a folder-only task.

Minimum acceptance target for the first story:

- Cargo workspace compiles.
- Root workspace uses Rust edition 2024 package crates and resolver 3.
- Rust binaries exist and print stable help output.
- TypeScript SDK package initializes under Bun and runs an empty or minimal `bun test` successfully.
- `just` and/or `cargo xtask` can run the initial validation commands.
- CLI golden-output fixture directory exists.
- Conformance fixture directory exists with README and initial fixture taxonomy.
- A thin protocol-neutral smoke path exists: either an envelope/domain-type round trip in Rust or a CLI/help plus core/proto execution path. Do **not** choose a gRPC smoke path until Step 4 resolves gRPC scope.

This prevents the first story from being purely technical scaffolding while still keeping feature implementation for later stories.

### Development Experience

- First scaffold must compile even before full features exist.
- CI should gate formatting, linting, tests, lockfile drift, docs, conformance fixture shape, and forbidden patterns as soon as practical.
- `zornmesh --help` and crate-level docs should exist early to keep CLI and API surfaces visible.
- The scaffold must make unsupported shortcuts hard: no Node package manager, no alternate Rust async runtime, no SQLite driver drift, no mock daemon in integration tests.

**Note:** Project initialization using this scaffold should be the first implementation story. Public starter templates may be referenced for conventions, but they are not a source of architectural authority.

### Starter Decision Risks

- **Risk:** Custom scaffold is slower than adopting a starter.  
  **Mitigation:** The project’s constraints are specific enough that retrofitting a generic starter would cost more and create hidden drift.

- **Risk:** Early scaffold becomes technical-only work.  
  **Mitigation:** The first story must produce a compileable scaffold plus at least one thin vertical smoke path and stable CLI help fixtures.

- **Risk:** Naming conflict (`zornmesh` vs `zorn` vs `zorn-meshd`) leaks into file/crate names.  
  **Mitigation:** Resolve binary/package naming in Step 4 architectural decisions before finalizing implementation story names or running scaffold commands.

## Core Architectural Decisions

### Decision Priority Analysis

**Critical Decisions (Block Implementation):**

1. **Product binary name:** The v0.1 installed command is `zornmesh`. The daemon is invoked as `zornmesh daemon`; `zorn`, `zorn-mesh`, and separate public `zorn-meshd` binaries are not v0.1 surfaces. Internal crate/service naming may still use `zornmesh-daemon`, but user-facing docs, fixtures, and release artifacts use `zornmesh`.
2. **Wire framing:** The internal mesh data plane uses `[length:u32 BE][frame_type:u8][payload]`, not LSP-style `Content-Length`. The `length` field covers `frame_type + payload`; parsers enforce the length cap before allocation; unknown frame types produce stable protocol errors.
3. **Protocol tiering:** ACK/NACK, stream lifecycle, ping/pong, cancellation, flow control, and capability probes use a dedicated control-frame tier rather than being modeled only as normal envelopes.
4. **ACK taxonomy:** Architecture distinguishes transport ACK, durable ACK, and delivery ACK. Transport ACK means a frame was received and syntactically accepted. Durable ACK means the relevant message state committed to SQLite. Delivery ACK means a consumer processed or accepted the message. SDK retry behavior must document which ACK it observes.
5. **gRPC scope:** No public or agent-facing gRPC data plane ships in v0.1. gRPC is allowed only as an OpenTelemetry/OTLP export transport. This explicitly supersedes older documents that mention gRPC as an agent-facing fallback.
6. **MCP version target:** MCP compatibility targets protocol version `2025-11-25`, verified during Step 4 as the current stable MCP protocol version. Conformance fixtures must pin the exact initialize/request/response shapes used, so the implementation does not drift with ambiguous "latest" language.
7. **AgentCard profile:** v0.1 targets the A2A `1.0.0` AgentCard profile, verified during Step 4. The phrase "current stable" means "A2A 1.0.0 as of this architecture decision," not a floating dependency. Older AgentCard shapes may be normalized at ingress; for audit/debugging, store both the raw input and canonical normalized form when normalization occurs.
8. **TypeScript runtime/support boundary:** First-party TypeScript SDK tooling and tests use Bun only. The v0.1 TypeScript SDK officially supports Bun as its runtime target unless a later architecture decision expands support. CI rejects Node/npm/pnpm/yarn drift in the first-party SDK workspace.
9. **Schema ownership:** TypeBox JSON Schema owns capability contract validation. Protobuf owns canonical internal envelope, registry, persistence, and audit models. JSON-RPC remains the external method envelope where applicable. These layers do not generate each other; they meet at UDS ingress and are tied together by conformance fixtures.
10. **Persistence implementation:** SQLite via `sqlx` is the only persistence engine. The architecture uses a single writer pool, reader pool, WAL, forward-only migrations, and ACK-after-commit durability. Writer pool size, busy timeout, checkpoint policy, migration locking, and crash-recovery behavior must be made explicit in implementation stories.
11. **Metrics exposure:** No TCP listener is enabled by default. Prometheus `/metrics` is exposed only through an explicit opt-in loopback gateway via command/config, with token/session protection. Binding to `0.0.0.0` or non-loopback interfaces is forbidden unless a future explicit architecture decision allows it.
12. **CLI output contracts:** CLI JSON, NDJSON, stderr wording, and exit codes are architectural contracts backed by fixtures before feature work expands.
13. **Fixture and conformance ownership:** Conformance fixtures are executable requirements. Protocol frames, MCP, AgentCard, error registry, CLI JSON/NDJSON, and security defaults must have fixture owners and pass/fail criteria before epics are treated as implementation-ready.
14. **Local UI scope:** v0.1 ships a local web companion UI launched only by `zornmesh ui`. It is not hosted, not LAN/public, not accounts/teams, not full chat, not a workflow editor, and not a cloud dashboard.
15. **UI process and trust boundary:** `zornmesh ui` starts or connects to the daemon, starts a loopback-only UI/API listener for that session, creates a per-launch session token, opens the browser unless disabled, and can print the protected loopback URL. The UI is never started as a side effect of SDK `connect()`.
16. **Daemon-owned UI API/live transport:** UI data comes from daemon-owned read APIs and a daemon-owned WebSocket/SSE live channel. The browser never opens SQLite directly and never speaks UDS directly. Roster, timeline, trace detail, send, broadcast, per-recipient outcomes, reconnect/backfill, and trust status reuse daemon schemas and redaction/audit rules.
17. **UI security and asset posture:** UI/API listener binds loopback only, refuses public/non-loopback binds with named errors, validates session token, CSRF token, Origin and Host on state-changing requests, protects WebSocket/SSE handshakes, redacts tokens from logs/history, and serves only bundled local assets.
18. **UI fixture ownership:** Browser E2E, offline asset blocking, external request blocking, responsive layouts, keyboard-only flows, screen-reader checks, reduced motion, and WCAG AA evidence are release-gating fixtures for Epic 6.

**Important Decisions (Shape Architecture):**

1. **Envelope sizing:** Treat 1 MiB as the normal non-stream payload acceptance limit; 8 MiB is a parser/safety ceiling that must not become the default happy path. Larger payloads use streaming with 256 KiB chunks. Rejection errors, reassembly limits, and per-stream quotas must be explicit.
2. **Error registry:** Stable product error codes live in `zornmesh-core` and are consumed by daemon, CLI, Rust SDK, TypeScript SDK, and conformance fixtures.
3. **Socket/rendezvous policy:** Socket path, lock path, PID ownership, readiness signaling, foreground behavior, signal handling, and restart semantics are part of the SDK/daemon contract, not daemon internals.
4. **Conformance taxonomy:** Protocol, persistence, security, observability, CLI, AgentCard, MCP, and meta-fixture checks all live under a shared conformance corpus.
5. **Release artifact identity:** Public artifacts use `zornmesh` naming; internal crates remain `zornmesh-*`.
6. **CLI UX as product surface:** `zornmesh trace` is the hero forensic workflow. `doctor`, `tail`, `inspect`, `replay`, and `audit verify` must have stable human and machine affordances.
7. **Doctor coverage:** `zornmesh doctor` should eventually check daemon health, socket permissions, metrics gateway status, token/session state, queue pressure, payload drops/rejections, SQLite writability, audit-log writability, and retention/redaction health.
8. **Decision traceability:** Each critical decision must map to affected FR/NFR coverage before implementation readiness is rerun.

**Deferred Decisions (Post-MVP):**

1. Public gRPC data plane.
2. A2A task/RPC implementation beyond AgentCard-compatible identity metadata.
3. Python SDK implementation.
4. Hosted/cloud dashboard, LAN/public console, accounts/teams, full chat workspace, workflow editor, remote browser assets, and external runtime services.
5. Distributed or multi-host federation.
6. Remote configuration.
7. Custom encryption-at-rest layer beyond OS/filesystem protections and documented redaction/deletion semantics.
8. Node runtime support for the TypeScript SDK.

### Data Architecture

**Database and storage engine:**

- SQLite is the only v0.1 database.
- Rust access uses `sqlx`; Step 4 verification confirms `sqlx` `0.8.6` as the current crate version.
- No `rusqlite`, external broker database, embedded key-value store, Redis, NATS, Kafka, RabbitMQ, or cloud persistence layer is allowed in core v0.1.

**Pool and pragma model:**

- One writer pool owns all mutating transactions. Implementation stories must clarify whether this is a literal size-1 pool or a single writer actor over a constrained pool.
- Reader pool serves inspection, trace, replay, and CLI query paths.
- WAL is required.
- Writer durability uses `synchronous=FULL`.
- Reader connections may use read-optimized settings but cannot weaken writer durability.
- Foreign keys and migration version checks are mandatory at startup.
- Busy timeout, WAL checkpoint policy, startup recovery, migration lock behavior, disk-full behavior, and crash windows must be specified in persistence stories.

**Core data model:**

Required persistence domains:

- agents and AgentCard metadata, including raw and normalized forms when normalization occurs
- capability descriptors and schema versions
- envelopes/messages
- delivery leases
- idempotency keys
- subscriptions
- stream chunks/state
- dead-letter queue entries
- audit log/hash chain
- trace/correlation indexes
- retention tombstones/redaction markers
- schema migration state

**Validation strategy:**

- Capability descriptors are validated with TypeBox-generated JSON Schema.
- Internal canonical structures are modeled in Protobuf and consumed through generated bindings where needed.
- JSON-RPC request/response envelopes are validated at ingress before broker processing.
- Invalid schema, unsupported version, oversized payload, malformed frame, unknown frame type, unsupported MCP version, and unsupported AgentCard version errors must use stable error codes.

Step 4 version checks:

- `@sinclair/typebox`: `0.34.49`
- `@bufbuild/protobuf`: `2.12.0`

**Migration approach:**

- Migrations are forward-only.
- Destructive migrations require explicit architecture/story approval.
- Startup refuses unknown future schema versions.
- Migration failures surface through daemon startup errors and `zornmesh doctor`.
- Migration locks must prevent two concurrently started daemons from racing schema changes.

**Caching strategy:**

- No external cache.
- In-memory caches are allowed for registry lookups, subscription matchers, route indexes, and capability schema lookup.
- SQLite remains source of truth.
- Caches must rebuild deterministically after daemon restart.
- Cache invalidation rules must be observable enough for `doctor` and trace reconstruction to explain stale or rejected routes.

### Authentication & Security

**Authentication model:**

- v0.1 trust anchor is local OS identity through UDS peer credentials on Unix and equivalent local named-pipe credentials on supported platforms.
- Socket permissions must reject world/group-readable unsafe configurations.
- Daemon refuses root/admin execution unless a future explicitly approved mode changes that.
- The daemon help and diagnostics must make the default clear: local-only, no default TCP listener, no remote access.

**Authorization model:**

- Capability access is default-deny for high-privilege operations.
- High-privilege capabilities require explicit local allowlist policy.
- Authorization checks occur before message dispatch and before persistence side effects that imply accepted delivery.
- AgentCard capability metadata is not itself authorization; it is identity/capability description input to policy.

**Transport security:**

- Core v0.1 data plane is local IPC only.
- No default TCP listener exists.
- Opt-in loopback metrics gateway may expose `/metrics` only when explicitly enabled through `zornmesh metrics serve` or local config, protected by token/session controls.
- Metrics gateway binds loopback only by default and must reject non-loopback bind attempts unless a future decision explicitly permits them.
- No remote configuration fetch is allowed.

**Data protection:**

- Secret redaction is mandatory in logs, traces, CLI output, diagnostics, and evidence bundles.
- Personal-data deletion/redaction must preserve audit correlation and hash-chain semantics.
- v0.1 relies on OS/filesystem protection for SQLite at rest; custom database encryption is deferred.

**Release integrity:**

- Sigstore signatures and CycloneDX SBOMs are release-blocking.
- `zornmesh inspect sbom` and release verification flows are part of the operator surface.

### API & Communication Patterns

**CLI and binary surface:**

- Public command: `zornmesh`.
- Daemon command: `zornmesh daemon`.
- Short aliases such as `zorn` are deferred until usage data justifies them.
- CLI help fixtures must use `zornmesh`.
- `zornmesh daemon --help` must clearly communicate local-only defaults and that no TCP listener starts unless explicitly requested.

**Internal mesh protocol:**

- Frame format: `[length:u32 BE][frame_type:u8][payload]`.
- `length` includes `frame_type + payload`.
- Frame types distinguish normal JSON-RPC/envelope payloads from control frames.
- Control frames cover ACK, NACK, cancellation, stream lifecycle, ping/pong, flow control, and capability probes.
- Frame parser limits are enforced before allocation or JSON/Protobuf decoding.
- Unknown frame types, partial reads, invalid lengths, oversized frames, invalid payload encodings, and unsupported versions return stable protocol errors.
- Reserved frame-type ranges must be documented before implementation.
- Payload encoding is frame-type-specific and must be represented in conformance fixtures.
- Protocol version/capability negotiation occurs during initialize/handshake, not by guessing from frame payloads.

**ACK semantics:**

- Transport ACK: frame received and syntactically accepted.
- Durable ACK: message state committed to SQLite according to the active delivery mode.
- Delivery ACK: consumer processed or accepted the delivered message.
- NACKs must identify which layer failed when safe to disclose.
- SDKs must document retry behavior for each ACK layer.
- Crash window tests must cover commit succeeds / ACK fails and ACK sent / consumer fails cases.

**MCP bridge:**

- MCP compatibility targets protocol version `2025-11-25`.
- MCP stdio adapter speaks MCP framing/protocol externally.
- The internal mesh transport does not use MCP/LSP `Content-Length`; the adapter maps between MCP host protocol and zorn-mesh internal frames.
- Project-context examples referencing older MCP versions should be corrected downstream.
- Required fixtures: `fixtures/mcp/2025-11-25/*.json` or equivalent conformance paths for initialize, capability negotiation, graceful degradation, and error behavior.

**AgentCard/A2A identity:**

- Agent identity metadata targets A2A `1.0.0` AgentCard profile.
- v0.1 implements AgentCard-compatible identity/capability registration, not full A2A task transport.
- Older AgentCard shapes may be normalized at ingress.
- When normalization occurs, persistence stores raw input and canonical normalized form for audit/debugging.
- Required fixtures: `fixtures/a2a/agentcard-1.0.0/*.json` for canonical, invalid, legacy-normalized, and high-privilege examples.

**gRPC decision:**

- No public or SDK data-plane gRPC transport in v0.1.
- gRPC is allowed only for OTLP export.
- Any future gRPC data-plane work requires a new architecture decision and conformance expansion.
- This decision supersedes older gRPC fallback references in brief/PRD/project-context history.

**Error handling:**

- Stable error code registry lives in `zornmesh-core`.
- A generated or checked fixture such as `fixtures/errors/registry.json` should represent the registry for SDK and CLI tests.
- JSON-RPC errors include stable code, category, message, retryability, and safe diagnostic data.
- CLI maps product errors to stable exit codes and stderr wording.
- SDKs expose equivalent error semantics across Rust and TypeScript.
- CI should reject product error codes not present in the registry.

**Streaming and backpressure:**

- Streaming uses explicit stream/control frames.
- Default chunk target is 256 KiB.
- Backpressure must surface within the PRD budget and propagate through SDK APIs.
- CLI streaming JSON output is NDJSON: one event per line, no mixed human text in JSON mode.
- Backpressure, payload-limit, stream-quota, and reassembly-limit errors must explain what happened, user impact, and next diagnostic action.

**Observability communication:**

- W3C tracecontext is propagated through envelopes.
- OpenTelemetry is mandatory.
- Step 4 verification confirms OpenTelemetry semantic conventions `v1.34.0` / Rust `opentelemetry-semantic-conventions` `0.31.0`.
- OTLP export is disabled by default.
- Prometheus metrics are opt-in loopback only, not always-on.

### Frontend Architecture

v0.1 ships a local web companion UI launched explicitly by `zornmesh ui`.

The v0.1 UI architecture is:

- **Browser app:** Bun-managed React/Next.js app under `apps/local-ui`, using Tailwind-aligned tokens and Radix-style accessible primitives through project-owned component wrappers.
- **Serving model:** release artifacts include bundled local UI assets. The daemon/UI gateway serves those assets only on an explicit `zornmesh ui` loopback session. No CDN scripts, remote fonts, analytics, remote images, or remote config are permitted at runtime.
- **Launch model:** `zornmesh ui [--no-open] [--print-url] [--no-input]` starts or connects to the daemon, binds a loopback-only UI/API listener, creates a per-launch protected session, opens the default browser unless disabled, and can print a protected loopback URL.
- **API model:** the browser consumes daemon-owned UI APIs for roster, timeline, trace detail, direct send, broadcast, per-recipient outcomes, reconnect/backfill, local trust state, and CLI handoff commands. UI APIs reuse daemon redaction, audit, trace, delivery-state, and authorization semantics.
- **Live updates:** WebSocket or SSE is permitted only through the protected loopback UI session. Live updates are ordered by daemon sequence; browser receipt time is secondary diagnostic metadata. Reconnect performs daemon-sequence backfill before marking the view current.
- **Security model:** loopback-only bind, per-session token, CSRF protection, Origin/Host validation, protected WebSocket/SSE handshake, token redaction, no direct SQLite access, no direct UDS access from the browser, and fail-closed state-changing requests.
- **User experience scope:** Live Mesh Control Room, Focus Trace Reader, safe direct composer, safe broadcast confirmation, per-recipient outcome list, daemon/local trust status, and CLI handoff copy blocks.
- **Out of scope:** hosted/cloud dashboard, LAN/public console, accounts/teams, rich chat workspace, workflow editor, remote browser assets, and external runtime services.

The CLI remains the durable automation, recovery, and offline audit surface. The TypeScript SDK remains a Bun-supported SDK package under `sdks/typescript`; it is separate from the browser app.

### CLI UX Contract Additions

**Hero workflow:**

- `zornmesh trace <correlation_id>` is the hero forensic workflow.
- Trace not-found output must be humane and actionable: suggest checking ID spelling, time window, `tail`, `inspect`, DLQ, or `doctor` as appropriate.
- JSON trace output must remain machine-stable and never include human prose outside fields.

**Mode separation:**

- Human output and JSON output must never mix on stdout.
- Errors go to stderr.
- Streaming JSON is NDJSON: exactly one event per line.
- Non-TTY and `NO_COLOR=1` disable ANSI.

**Required early fixture families:**

- `fixtures/frames/*.bin`
- `fixtures/cli/help/*.txt`
- `fixtures/cli/json/*.golden`
- `fixtures/cli/ndjson/*.golden`
- `fixtures/cli/stderr/*.txt`
- `fixtures/errors/registry.json`
- `fixtures/mcp/2025-11-25/*.json`
- `fixtures/a2a/agentcard-1.0.0/*.json`

**Specific CLI states to fixture:**

- root help
- trace help
- trace success JSON
- trace not found human output
- trace not found stderr
- payload too large
- stream quota exceeded
- backpressure surfaced
- daemon local-only help
- doctor healthy JSON
- doctor metrics gateway disabled
- doctor metrics gateway enabled
- doctor audit-log unwritable
- tail NDJSON example

### Infrastructure & Deployment

**Workspace and build:**

- Root Cargo workspace uses Rust edition 2024 and resolver `3`.
- Step 4 inherits Step 3 verified Rust crate versions:
  - `tokio` `1.52.1`
  - `clap` `4.6.1`
  - `sqlx` `0.8.6`
- Bun remains the TypeScript runtime/package/test tool; Step 3 verified Bun `1.3.13`.

**CI/CD:**

- Rust CI gates formatting, linting, tests, docs, coverage, and conformance fixtures.
- TypeScript CI uses Bun only and should run `bun install --frozen-lockfile` once the lockfile exists, then `bun test`.
- CI rejects forbidden runtime/tool drift: Node/npm/pnpm/yarn for TS and alternate Rust async runtimes.
- Lockfile drift is a CI failure once lockfiles exist.
- CI should include explicit negative tests proving no default TCP listener starts.

**Configuration:**

- Precedence: CLI flags override environment variables; environment variables override local config files; local config files override defaults.
- Remote config is forbidden.
- Config must expose daemon mode, socket paths, metrics opt-in, OTel export, retention, and high-privilege allowlists.

**Packaging and release:**

- Public release artifact naming uses `zornmesh`.
- Release artifacts include signed binaries and SBOMs.
- Linux/macOS are v0.1 targets.
- Docker is not a runtime dependency for core product use.

**Monitoring and logging:**

- Structured logs are local and secret-redacted.
- OTel traces/metrics are available but export is disabled by default.
- `/metrics` requires explicit opt-in loopback gateway.
- `zornmesh doctor` is the primary operator diagnostic path.

**Scaling strategy:**

- v0.1 is single-machine and single-user scoped.
- No distributed broker/federation.
- Subject/subscription caps, payload caps, stream chunking, retention, and metric cardinality caps are enforced as architectural invariants.

### Decision Impact Analysis

**Implementation Sequence:**

1. Scaffold workspace using `zornmesh` naming and Rust 2024 resolver `3`.
2. Establish `zornmesh-core` error codes, IDs, envelope types, ACK taxonomy, version constants, and schema boundaries.
3. Implement frame parser/encoder with frame-type discrimination, strict length semantics, reserved frame ranges, and frame fixtures.
4. Implement SQLite `sqlx` migrations, writer/reader pools, durability pragmas, busy timeout, migration locks, checkpoint policy, and crash-window tests.
5. Implement daemon lifecycle/rendezvous with local socket trust checks, lock/PID ownership, foreground behavior, signal handling, readiness line, and no-default-TCP tests.
6. Implement CLI help/output fixtures for `zornmesh`, including daemon local-only messaging and trace empty states.
7. Implement MCP adapter against protocol version `2025-11-25` with pinned fixtures.
8. Implement AgentCard A2A `1.0.0` canonical profile, legacy normalization, and raw/canonical persistence.
9. Implement observability foundations with OTel and opt-in loopback metrics gateway.
10. Expand SDK parity through shared conformance fixtures.

**Cross-Component Dependencies:**

- Binary naming affects CLI fixtures, docs, release artifacts, socket names, and implementation stories.
- Frame format affects SDKs, daemon, broker, conformance, MCP adapter, and replay tooling.
- ACK taxonomy affects persistence, SDK retries, broker delivery state, DLQ, and trace reconstruction.
- Error registry affects every crate and SDK.
- SQLite schema affects delivery semantics, replay, audit, retention, compliance export, and CLI inspection.
- Metrics gateway decision affects security, observability, docs, doctor checks, and deployment defaults.
- AgentCard/MCP version pins affect protocol fixtures, compatibility tests, docs, and host integration stories.
- Bun-only TypeScript support affects CI, SDK packaging, docs, and contributor setup.
- CLI fixture taxonomy affects epics/stories because user-facing behavior is no longer left to implementation interpretation.

### Decision Traceability Notes

| Decision Area | Primary FR/NFR Impact |
|---|---|
| `zornmesh` binary and daemon command | FR15-FR21, FR45-FR48, NFR-C5, NFR-O4 |
| Frame format and control-frame tier | FR1-FR10, FR33-FR35, NFR-P2-NFR-P6, NFR-S7, NFR-C3-C4 |
| ACK taxonomy and SQLite durability | FR2-FR10, FR22-FR27, NFR-R2-R6, NFR-P3 |
| No public gRPC data plane | FR33-FR35, NFR-S1, NFR-C4 |
| MCP `2025-11-25` target | FR33-FR35, NFR-C4 |
| A2A `1.0.0` AgentCard target | FR11-FR14, FR34, NFR-S6, NFR-C4 |
| Bun-only TypeScript SDK support | FR61-FR62, NFR-C2, NFR-M1-M6 |
| TypeBox/Protobuf schema split | FR11-FR14, FR61-FR62, NFR-S7, NFR-M4 |
| sqlx SQLite persistence | FR22-FR28, FR41-FR44, NFR-R2-R6, NFR-CA1-CA5 |
| Opt-in loopback metrics | FR29-FR32, NFR-S1, NFR-O1-O4 |
| CLI fixture-backed UX contracts | FR21, FR24-FR28, FR31, FR45-FR48, NFR-C5, NFR-O4 |
| Local UI launch/API/live gateway | FR49-FR60, NFR-P7-NFR-P9, NFR-S1, NFR-S9-NFR-S12, NFR-R7 |
| Local UI browser app/assets/fixtures | FR49-FR60, NFR-C6-NFR-C7, NFR-M7, NFR-A11Y1-NFR-A11Y6 |
| Error registry and conformance fixtures | All FRs through acceptance fixtures, NFR-M2-M5 |

## Implementation Patterns & Consistency Rules

### Pattern Categories Defined

**Critical Conflict Points Identified:** 42 areas where AI agents could otherwise diverge across naming, file placement, schema formats, protocol semantics, error handling, CLI output, persistence, observability, and test fixtures.

These rules are not feature implementation details. They are consistency constraints so independently implemented stories compose into one product.

### Naming Patterns

**Product and Artifact Naming:**

- Public command: `zornmesh`.
- Daemon subcommand: `zornmesh daemon`.
- Rust crates: `zornmesh-core`, `zornmesh-proto`, `zornmesh-store`, `zornmesh-rpc`, `zornmesh-broker`, `zornmesh-sdk`, `zornmesh-daemon`, `zornmesh-cli`.
- Rust package names use kebab-case; Rust crate imports use underscore form, e.g. `zornmesh_core`.
- Public release artifacts use `zornmesh`, not `zorn`, `zorn-mesh`, or `zorn-meshd`.
- Docs and fixtures must not introduce alternate product command names.

**Database Naming Conventions:**

- Tables use plural `snake_case`: `agents`, `capabilities`, `messages`, `delivery_leases`, `audit_entries`.
- Columns use `snake_case`: `agent_id`, `correlation_id`, `created_at_unix_ns`.
- Primary keys use `id` only when the table has a single native ID; otherwise use typed IDs such as `agent_id`, `message_id`, `lease_id`.
- Foreign keys use `<referenced_entity>_id`, e.g. `agent_id`, `message_id`, `subscription_id`.
- Indexes use `idx_<table>__<columns>`: `idx_messages__correlation_id`.
- Unique indexes use `uq_<table>__<columns>`.
- Foreign key constraints use `fk_<from_table>__<to_table>`.
- Timestamps stored in SQLite use integer Unix nanoseconds with `_unix_ns` suffix. JSON/CLI output renders RFC3339 UTC strings.

**Protocol and API Naming Conventions:**

- MCP-standard method names remain unchanged.
- Zorn-specific JSON-RPC methods use `zornmesh.<domain>.<action>`, e.g. `zornmesh.message.publish`, `zornmesh.trace.get`.
- JSON wire fields use `snake_case`: `trace_id`, `span_id`, `correlation_id`, `delivery_mode`, `idempotency_key`.
- Subject names use lowercase dot-delimited tokens: `agent.task.started`.
- Subject wildcards, once implemented, must be documented in one matcher spec and reused by all SDKs.
- Frame type constants use stable PascalCase enum names in Rust and string fixture names in conformance docs, e.g. `Envelope`, `Ack`, `Nack`, `Ping`, `Pong`, `StreamChunk`, `StreamEnd`.

**Code Naming Conventions:**

- Rust modules/functions/variables: `snake_case`.
- Rust types/traits/enums: `UpperCamelCase`.
- Rust constants/statics: `SCREAMING_SNAKE_CASE`.
- TypeScript files: kebab-case, e.g. `agent-card.ts`, `trace-client.ts`.
- TypeScript exported types/classes: `UpperCamelCase`.
- TypeScript functions/variables: `camelCase`.
- TypeScript serialization boundary must map to wire `snake_case`; language-native names cannot leak into wire fixtures.
- Product error codes use `ZM_<DOMAIN>_<SLUG>` string form, e.g. `ZM_PROTOCOL_UNKNOWN_FRAME_TYPE`. JSON-RPC numeric codes are registry metadata, not the stable human-facing product code.

### Structure Patterns

**Project Organization:**

- Product Rust crates live under `crates/<crate-name>/`.
- Cross-language SDKs live under `sdks/<language>/`.
- Shared conformance fixtures live under `conformance/`.
- CLI golden output fixtures live under `fixtures/cli/`.
- Binary frame fixtures live under `fixtures/frames/`.
- Error registry fixture lives under `fixtures/errors/registry.json`.
- MCP fixtures live under `fixtures/mcp/2025-11-25/`.
- AgentCard fixtures live under `fixtures/a2a/agentcard-1.0.0/`.
- SQLite migrations live under `migrations/`.
- Test orchestration assets live under `test-infra/`.
- User/operator docs live under `docs/`.

**Rust Test Placement:**

- Unit tests for pure functions live co-located in the defining module under `#[cfg(test)]`.
- Crate integration tests live under `crates/<crate>/tests/`.
- Cross-crate daemon/SDK integration tests live under a workspace-level integration harness or `test-infra/`, not hidden inside one product crate.
- Conformance tests consume fixtures; they must not generate canonical fixtures implicitly during test execution.
- Property/chaos/concurrency tests are named by behavior, not implementation detail.

**TypeScript Test Placement:**

- SDK unit tests live near source as `*.test.ts` or under `sdks/typescript/tests/`, but conformance tests must be grouped under `sdks/typescript/tests/conformance/`.
- TypeScript tests use `bun test`.
- No Node-specific polyfill, runner, or package-manager file is allowed unless a later architecture decision changes runtime support.

**Crate Ownership Boundaries:**

- `zornmesh-core`: IDs, error registry types, envelope domain types, ACK taxonomy, version constants.
- `zornmesh-proto`: Protobuf schemas, generated bindings integration, binary vector helpers.
- `zornmesh-store`: SQLite schema, migrations, writer/reader access, persistence recovery.
- `zornmesh-rpc`: UDS/named-pipe transport, frame parser/encoder, JSON-RPC/MCP handshake, gateway boundaries.
- `zornmesh-broker`: routing, subject matching, delivery state machines, leases, backpressure.
- `zornmesh-sdk`: Rust SDK connect/register/publish/subscribe/request APIs.
- `zornmesh-daemon`: daemon process lifecycle, readiness, signal handling, orchestration.
- `zornmesh-cli`: command parsing, human/JSON output, stderr/exit-code mapping.
- `xtask`: generation, fixture validation, conformance orchestration, release preflight.

**File Structure Patterns:**

- Generated files must be clearly marked and isolated under generated output paths or modules.
- Hand-authored schemas live beside their source-of-truth layer: TypeBox for capability schemas, `.proto` files for internal canonical models.
- Platform-specific code is isolated behind narrow modules such as `platform/unix.rs` and `platform/macos.rs`.
- Unsafe code, if required for platform syscalls, is isolated and documented at the smallest possible boundary.

### Format Patterns

**JSON-RPC and Wire Format Rules:**

- All external JSON-RPC messages use JSON-RPC 2.0 shape.
- Zorn-specific extension fields use `snake_case`.
- JSON object field order is not semantically meaningful; fixtures may format deterministically for readability.
- Unsupported versions, unknown methods, validation failures, payload size failures, and authorization failures all return registered product error codes.

**CLI JSON Response Formats:**

Read commands returning JSON use a stable top-level object:

```json
{
  "schema_version": "1.0.0",
  "generated_at": "2026-04-27T00:00:00Z",
  "data": {},
  "warnings": []
}
```

Streaming commands with `--output json` emit NDJSON, one event per line:

```json
{"schema_version":"1.0.0","event_type":"trace.span_started","sequence":1,"data":{}}
{"schema_version":"1.0.0","event_type":"trace.span_finished","sequence":2,"data":{}}
```

Rules:

- Human prose never appears on stdout in JSON mode.
- Errors go to stderr.
- Warnings in JSON mode use structured `warnings`; warnings in human mode may appear on stderr.
- Empty states are explicit and stable, not omitted.

**Error Format:**

Product errors use this logical shape across CLI, SDKs, and JSON-RPC:

```json
{
  "code": "ZM_PROTOCOL_UNKNOWN_FRAME_TYPE",
  "category": "protocol",
  "message": "Unknown frame type.",
  "retryable": false,
  "safe_details": {}
}
```

Rules:

- `message` is safe to display.
- Sensitive values never appear in `message` or `safe_details`.
- `retryable` must reflect the layer that failed.
- SDKs may wrap errors idiomatically, but must expose `code`, `category`, and `retryable`.

**Data Exchange Formats:**

- JSON field names: `snake_case`.
- Timestamps in JSON: RFC3339 UTC strings.
- Timestamps in SQLite: integer Unix nanoseconds.
- Durations in JSON: integer milliseconds with `_ms` suffix unless nanosecond precision is required.
- Byte sizes in JSON: integer bytes with `_bytes` suffix.
- IDs are strings.
- Trace IDs and span IDs use W3C tracecontext-compatible lowercase hex.
- Correlation IDs are strings and must remain stable across replay/audit views.
- Nullable fields are used only when absence and explicit null are semantically different; otherwise omit optional fields.

### Communication Patterns

**Frame and Control Event Patterns:**

- Frame parser behavior is owned by `zornmesh-rpc`.
- Delivery state behavior is owned by `zornmesh-broker`.
- Persistence ACK behavior is owned by `zornmesh-store`.
- No crate may invent new frame types without updating the frame registry, fixtures, and conformance tests.
- Control-frame payloads must include enough correlation to map back to message/stream state without parsing logs.

**Event Naming and Payloads:**

- Internal event names use lowercase dot-delimited form: `message.accepted`, `message.durable_ack`, `message.delivery_ack`, `stream.chunk_sent`, `stream.backpressure_applied`.
- Event payloads use `snake_case`.
- Event payloads include `trace_id` and `correlation_id` where available.
- Event schema changes require versioned fixtures.
- Events that may appear in CLI NDJSON require fixture coverage before shipping.

**Logging and Observability Patterns:**

- Log fields use `snake_case`.
- Log levels use lowercase: `trace`, `debug`, `info`, `warn`, `error`.
- Span names use `zornmesh.<component>.<operation>`, e.g. `zornmesh.broker.route`.
- OTel attributes use a `zornmesh.` prefix for product-specific fields.
- High-cardinality values must not become metric labels.
- Secret redaction occurs before logging/tracing, not during display.

**Configuration Communication:**

- Precedence is always CLI flag > environment variable > local config file > default.
- Remote config is forbidden.
- Any command that starts a listener or exporter must say so explicitly in help and diagnostics.

### Process Patterns

**Error Handling Patterns:**

- Domain errors originate from typed error enums, not ad-hoc strings.
- Library crates return typed errors; binaries map them to CLI/JSON-RPC presentation.
- No broad catch-and-success fallback.
- No string matching on error messages.
- Every externally visible error maps to a registry entry.
- Retry logic is centralized and deadline-aware; agents must not implement local retry loops that bypass idempotency rules.
- NACKs identify transport, validation, authorization, persistence, or delivery layer when safe.

**Validation Patterns:**

- Validate at ingress before broker side effects.
- Validate frame length before allocation.
- Validate JSON-RPC shape before method dispatch.
- Validate Protobuf/internal envelope before persistence.
- Validate capability payload against registered JSON Schema before delivery.
- Validation failures are observable through error code, trace, and CLI diagnostics.

**Daemon Lifecycle Patterns:**

- Startup readiness uses one parseable readiness line and/or a machine-readable readiness signal.
- Socket lock/PID ownership is authoritative for single-daemon enforcement.
- Foreground/managed modes must be explicit.
- SIGTERM/SIGINT trigger graceful drain according to the shutdown budget.
- Crash recovery must be testable without relying on sleeps.
- Default startup must not open TCP listeners.

**CLI UX Patterns:**

- Success human output goes to stdout.
- Errors go to stderr.
- `--output json` output is stable and machine-only.
- Streaming JSON is NDJSON.
- Human empty states include next actions.
- Non-TTY and `NO_COLOR=1` disable ANSI.
- `--no-input` forbids prompts and fails with a stable error if input would be required.
- Exit codes are stable and documented in the error registry or CLI reference.

**Persistence Process Patterns:**

- Mutations that imply durable acceptance use the writer path.
- Read-only CLI inspection uses reader path.
- ACK-after-commit means durable ACK is emitted only after the relevant commit succeeds.
- Retention and redaction jobs produce audit evidence.
- Disk-full/read-degraded behavior must preserve inspection where possible and clearly reject unsafe writes.

**Loading/State Patterns:**

Zorn Mesh uses shared operational state names across daemon, CLI, SDKs, and the local UI. The UI may present user-friendly labels, but the underlying API values must remain stable and fixture-covered:

- daemon: `starting`, `ready`, `draining`, `stopped`, `unhealthy`
- connection: `connecting`, `connected`, `retrying`, `failed`
- stream: `open`, `backpressured`, `closing`, `closed`, `failed`
- delivery: `accepted`, `durable_ack`, `leased`, `delivery_ack`, `nack`, `dead_lettered`
- ui session: `starting`, `ready`, `expired`, `reconnecting`, `backfilling`, `stale`, `failed`
- ui view freshness: `current`, `backfilling`, `stale`, `disconnected`
- trace completeness: `complete`, `partial`, `retention_gap`, `backfill_pending`
- broadcast outcome: `pending`, `partial_success`, `all_failed`, `success`, `stale`

These states must use the same names in logs, traces, CLI JSON, and SDK status APIs unless a layer has a documented reason to map them.

### Enforcement Guidelines

**All AI Agents MUST:**

- Reuse the named crate ownership boundaries instead of inventing new cross-cutting modules.
- Add or update fixtures when changing wire, CLI, MCP, AgentCard, error, or persistence-visible behavior.
- Use `zornmesh` in all public command examples and fixtures.
- Preserve JSON `snake_case` at wire and CLI boundaries.
- Map external errors to registered product error codes.
- Keep human output and machine output separated.
- Preserve local-only/no-default-TCP behavior unless a future architecture decision changes it.
- Avoid Node/npm/pnpm/yarn in the TypeScript SDK workspace.
- Avoid alternate Rust async runtimes.
- Avoid direct SQLite access outside `zornmesh-store`.
- Avoid CLI code reading SQLite directly; CLI should use SDK/RPC/client boundaries.

**Pattern Enforcement:**

- `cargo xtask` should validate fixture shape, error registry consistency, forbidden tooling, and generated schema drift.
- CI should run conformance checks for protocol, CLI, errors, MCP, AgentCard, and persistence fixtures as those suites land.
- Pattern violations should be fixed in code or explicitly escalated as architecture changes; they should not be hidden in story notes.
- Pattern changes require updating architecture, fixtures, and affected downstream stories.

### Pattern Examples

**Good Examples:**

```rust
// Rust domain error exposes a registry-backed product code.
ErrorCode::new("ZM_PROTOCOL_UNKNOWN_FRAME_TYPE")
```

```json
{
  "schema_version": "1.0.0",
  "generated_at": "2026-04-27T00:00:00Z",
  "data": {
    "correlation_id": "01HV...",
    "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736"
  },
  "warnings": []
}
```

```text
fixtures/mcp/2025-11-25/initialize-success.json
fixtures/a2a/agentcard-1.0.0/canonical-basic.json
fixtures/errors/registry.json
fixtures/cli/stderr/trace-not-found.txt
```

```text
idx_messages__correlation_id
uq_agents__agent_id
fk_delivery_leases__messages
```

**Anti-Patterns:**

- Adding `zorn`, `zorn-mesh`, or `zorn-meshd` to user-facing command fixtures.
- Returning camelCase JSON at the CLI/wire boundary.
- Printing warnings or progress text to stdout during `--output json`.
- Opening a metrics TCP listener by default.
- Handling malformed frames after allocating the declared frame length.
- Emitting durable ACK before SQLite commit.
- Creating product error strings without registry entries.
- Letting `zornmesh-cli` query SQLite directly.
- Adding Node package-manager files to `sdks/typescript`.
- Creating conformance fixtures from implementation output without review.
- Mixing TypeBox capability schemas and Protobuf internal schemas as if one generated the other.

## Project Structure & Boundaries

### Complete Project Directory Structure

```text
zorn-mesh/
|-- Cargo.toml
|-- Cargo.lock
|-- rust-toolchain.toml
|-- rustfmt.toml
|-- clippy.toml
|-- justfile
|-- .editorconfig
|-- .gitignore
|-- .bun-version
|-- README.md
|-- LICENSE
|-- SECURITY.md
|-- CHANGELOG.md
|-- _bmad-output/
|   `-- planning-artifacts/
|       |-- prd.md
|       |-- architecture.md
|       `-- implementation-readiness-report-2026-04-27.md
|-- .github/
|   `-- workflows/
|       |-- ci.yml
|       |-- conformance.yml
|       `-- release.yml
|-- crates/
|   |-- zornmesh-core/
|   |   |-- Cargo.toml
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- ack.rs
|   |       |-- envelope.rs
|   |       |-- errors.rs
|   |       |-- ids.rs
|   |       |-- limits.rs
|   |       |-- state.rs
|   |       |-- time.rs
|   |       `-- versions.rs
|   |-- zornmesh-proto/
|   |   |-- Cargo.toml
|   |   |-- build.rs
|   |   |-- proto/
|   |   |   `-- zornmesh/
|   |   |       |-- audit.proto
|   |   |       |-- envelope.proto
|   |   |       `-- registry.proto
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- audit.rs
|   |       |-- envelope.rs
|   |       `-- registry.rs
|   |-- zornmesh-store/
|   |   |-- Cargo.toml
|   |   |-- tests/
|   |   |   |-- migrations.rs
|   |   |   |-- crash_recovery.rs
|   |   |   `-- retention.rs
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- audit.rs
|   |       |-- checkpoint.rs
|   |       |-- db.rs
|   |       |-- leases.rs
|   |       |-- messages.rs
|   |       |-- migrations.rs
|   |       |-- reader.rs
|   |       |-- redaction.rs
|   |       |-- retention.rs
|   |       `-- writer.rs
|   |-- zornmesh-rpc/
|   |   |-- Cargo.toml
|   |   |-- tests/
|   |   |   |-- framing.rs
|   |   |   |-- mcp_initialize.rs
|   |   |   `-- no_default_tcp.rs
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- frame.rs
|   |       |-- gateway.rs
|   |       |-- jsonrpc.rs
|   |       |-- mcp.rs
|   |       |-- metrics_gateway.rs
|   |       |-- ui_gateway.rs
|   |       |-- platform/
|   |       |   |-- mod.rs
|   |       |   |-- macos.rs
|   |       |   `-- unix.rs
|   |       `-- transport.rs
|   |-- zornmesh-broker/
|   |   |-- Cargo.toml
|   |   |-- tests/
|   |   |   |-- backpressure.rs
|   |   |   |-- delivery.rs
|   |   |   `-- subject_matching.rs
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- backpressure.rs
|   |       |-- delivery.rs
|   |       |-- idempotency.rs
|   |       |-- leases.rs
|   |       |-- registry.rs
|   |       |-- routing.rs
|   |       |-- streams.rs
|   |       `-- subjects.rs
|   |-- zornmesh-sdk/
|   |   |-- Cargo.toml
|   |   |-- tests/
|   |   |   |-- connect.rs
|   |   |   |-- conformance.rs
|   |   |   `-- retries.rs
|   |   `-- src/
|   |       |-- lib.rs
|   |       |-- agent.rs
|   |       |-- client.rs
|   |       |-- connect.rs
|   |       |-- publish.rs
|   |       |-- request.rs
|   |       |-- spawn.rs
|   |       |-- stream.rs
|   |       `-- subscribe.rs
|   |-- zornmesh-daemon/
|   |   |-- Cargo.toml
|   |   |-- tests/
|   |   |   |-- lifecycle.rs
|   |   |   |-- socket_ownership.rs
|   |   |   `-- shutdown.rs
|   |   `-- src/
|   |       |-- main.rs
|   |       |-- config.rs
|   |       |-- lifecycle.rs
|   |       |-- readiness.rs
|   |       |-- signals.rs
|   |       `-- supervisor.rs
|   `-- zornmesh-cli/
|       |-- Cargo.toml
|       |-- tests/
|       |   |-- cli_fixtures.rs
|       |   |-- exit_codes.rs
|       |   `-- json_output.rs
|       `-- src/
|           |-- main.rs
|           |-- output.rs
|           |-- exit_codes.rs
|           `-- commands/
|               |-- mod.rs
|               |-- agents.rs
|               |-- audit.rs
|               |-- daemon.rs
|               |-- doctor.rs
|               |-- inspect.rs
|               |-- replay.rs
|               |-- stdio.rs
|               |-- tail.rs
|               |-- trace.rs
|               `-- ui.rs
|-- xtask/
|   |-- Cargo.toml
|   `-- src/
|       |-- main.rs
|       |-- codegen.rs
|       |-- conformance.rs
|       |-- fixtures.rs
|       |-- forbidden_patterns.rs
|       `-- release.rs
|-- sdks/
|   |-- typescript/
|   |   |-- package.json
|   |   |-- bun.lock
|   |   |-- tsconfig.json
|   |   |-- README.md
|   |   |-- src/
|   |   |   |-- index.ts
|   |   |   |-- agent-card.ts
|   |   |   |-- client.ts
|   |   |   |-- connect.ts
|   |   |   |-- errors.ts
|   |   |   |-- publish.ts
|   |   |   |-- request.ts
|   |   |   |-- schema.ts
|   |   |   |-- stream.ts
|   |   |   `-- subscribe.ts
|   |   `-- tests/
|   |       |-- client.test.ts
|   |       |-- errors.test.ts
|   |       `-- conformance/
|   |           |-- agentcard.test.ts
|   |           |-- errors.test.ts
|   |           `-- protocol.test.ts
|   `-- python/
|       `-- README.md
|-- apps/
|   `-- local-ui/
|       |-- package.json
|       |-- bun.lock
|       |-- tsconfig.json
|       |-- next.config.ts
|       |-- tailwind.config.ts
|       |-- src/
|       |   |-- app/
|       |   |   |-- layout.tsx
|       |   |   `-- page.tsx
|       |   |-- components/
|       |   |   |-- live-agent-roster.tsx
|       |   |   |-- trace-timeline.tsx
|       |   |   |-- trace-detail.tsx
|       |   |   |-- safe-composer.tsx
|       |   |   `-- local-trust-status.tsx
|       |   |-- lib/
|       |   |   |-- api-client.ts
|       |   |   |-- state-taxonomy.ts
|       |   |   `-- redaction.ts
|       |   `-- tests/
|       |       |-- e2e/
|       |       |   |-- first-open.test.ts
|       |       |   |-- trace-inspection.test.ts
|       |       |   |-- safe-send.test.ts
|       |       |   `-- reconnect-backfill.test.ts
|       |       |-- accessibility/
|       |       |   `-- keyboard-and-screen-reader.test.ts
|       |       `-- external-requests.test.ts
|-- migrations/
|   |-- 0001_initial.sql
|   |-- 0002_audit_hash_chain.sql
|   `-- README.md
|-- conformance/
|   |-- README.md
|   |-- manifest.json
|   |-- protocol/
|   |   |-- frames.json
|   |   |-- envelope.json
|   |   `-- ack-taxonomy.json
|   |-- persistence/
|   |   |-- ack-after-commit.json
|   |   |-- retention.json
|   |   `-- crash-recovery.json
|   |-- security/
|   |   |-- socket-permissions.json
|   |   |-- no-default-tcp.json
|   |   `-- redaction.json
|   |-- observability/
|   |   |-- tracecontext.json
|   |   `-- metrics-cardinality.json
|   |-- cli/
|   |   |-- output-contracts.json
|   |   `-- exit-codes.json
|   |-- ui/
|   |   |-- launch-session.json
|   |   |-- live-roster.json
|   |   |-- trace-backfill.json
|   |   |-- safe-direct-send.json
|   |   |-- safe-broadcast.json
|   |   `-- external-request-blocking.json
|   `-- meta/
|       |-- forbidden-patterns.json
|       `-- fixture-schema.json
|-- fixtures/
|   |-- frames/
|   |   |-- envelope-basic.bin
|   |   |-- control-ack.bin
|   |   |-- control-nack.bin
|   |   `-- invalid-unknown-frame-type.bin
|   |-- cli/
|   |   |-- help/
|   |   |   |-- help-root.txt
|   |   |   |-- help-trace.txt
|   |   |   `-- help-daemon.txt
|   |   |-- json/
|   |   |   |-- trace-success.golden
|   |   |   |-- doctor-healthy.golden
|   |   |   `-- doctor-metrics-disabled.golden
|   |   |-- ndjson/
|   |   |   `-- tail-example.golden
|   |   `-- stderr/
|   |       |-- trace-not-found.txt
|   |       |-- payload-too-large.txt
|   |       `-- backpressure-surfaced.txt
|   |-- ui/
|   |   |-- roster-three-agents.json
|   |   |-- trace-with-retention-gap.json
|   |   |-- direct-send-outcomes.json
|   |   |-- broadcast-partial-failure.json
|   |   |-- reconnect-backfill.json
|   |   `-- local-trust-status.json
|   |-- errors/
|   |   `-- registry.json
|   |-- mcp/
|   |   `-- 2025-11-25/
|   |       |-- initialize-success.json
|   |       |-- initialize-unsupported-version.json
|   |       `-- graceful-degradation.json
|   `-- a2a/
|       `-- agentcard-1.0.0/
|           |-- canonical-basic.json
|           |-- canonical-high-privilege.json
|           |-- invalid-missing-id.json
|           `-- legacy-normalized.json
|-- test-infra/
|   |-- otel-collector.yaml
|   |-- process-harness/
|   |   `-- README.md
|   `-- fault-injection/
|       |-- disk-full.md
|       |-- crash-after-commit.md
|       `-- slow-consumer.md
`-- docs/
    |-- architecture/
    |   |-- framing.md
    |   |-- ack-taxonomy.md
    |   |-- persistence.md
    |   |-- security.md
    |   `-- observability.md
    |-- cli.md
    |-- configuration.md
    |-- jsonrpc-error-codes.md
    |-- mcp-compatibility.md
    |-- agentcard-profile.md
    |-- adapter-author-guide.md
    |-- release-integrity.md
    `-- compliance/
        |-- evidence-export.md
        |-- eu-ai-act-traceability.md
        `-- nist-ai-rmf-map.md
```

### Architectural Boundaries

**API Boundaries:**

- **CLI boundary:** `crates/zornmesh-cli` owns user-facing commands, human/JSON/NDJSON formatting, stderr wording, and exit-code mapping. It must not query SQLite directly.
- **Rust SDK boundary:** `crates/zornmesh-sdk` owns Rust application APIs: `connect`, register, publish, subscribe, request/reply, streaming, cancellation, idempotency, and timeouts.
- **TypeScript SDK boundary:** `sdks/typescript` owns Bun-supported TypeScript APIs and consumes shared conformance fixtures.
- **Daemon process boundary:** `crates/zornmesh-daemon` owns process startup, readiness, lock/PID/socket ownership, signal handling, graceful drain, and supervisor orchestration.
- **MCP boundary:** `crates/zornmesh-rpc::mcp` owns MCP `2025-11-25` stdio compatibility and maps MCP host behavior into internal frame/envelope semantics.
- **Metrics boundary:** `crates/zornmesh-rpc::metrics_gateway` owns opt-in loopback `/metrics`; no other component starts TCP listeners.
- **OTLP boundary:** observability export may use OTLP/gRPC; this does not create an agent-facing data-plane gRPC surface.
- **Local UI app boundary:** `apps/local-ui` owns browser presentation, interaction state, accessibility behavior, responsive layout, and UI fixtures. It must not open SQLite directly, speak UDS directly, bypass daemon redaction, or invent delivery/audit semantics.
- **UI gateway boundary:** `crates/zornmesh-rpc::ui_gateway` owns the explicit `zornmesh ui` loopback listener, static asset serving, session token lifecycle, CSRF/Origin/Host enforcement, WebSocket/SSE live channel, reconnect/backfill API, state-changing send/broadcast APIs, token redaction, and port-conflict errors.

**Component Boundaries:**

- `zornmesh-core` contains domain vocabulary and stable cross-crate types only. It does not open sockets, spawn processes, or query SQLite.
- `zornmesh-proto` owns internal Protobuf definitions and generated binding integration. It does not own capability JSON Schema.
- `zornmesh-rpc` owns framing, transport, JSON-RPC/MCP handshake, local socket details, and gateway boundaries. It does not own routing policy.
- `zornmesh-broker` owns routing, subject matching, delivery state, leases, streams, idempotency, and backpressure. It does not own storage schema.
- `zornmesh-store` owns SQLite and audit persistence. No other crate writes database tables directly.
- `zornmesh-cli` uses SDK/RPC/client boundaries, never store internals.
- `xtask` orchestrates generation and validation; it must not become product runtime code.

**Service Boundaries:**

There is one always-available local product service: the auto-spawned daemon. `zornmesh ui` may create an explicit loopback UI/API listener for the current local UI session, owned by the daemon/RPC boundary and protected by the UI session controls. It is not a default network listener and must terminate when the UI session is stopped or expires.

- SDKs communicate with the daemon over local IPC.
- CLI communicates through SDK/RPC client paths.
- Daemon coordinates broker, store, RPC, and observability.
- No external broker/service/database is required.

**Data Boundaries:**

- SQLite access is isolated in `zornmesh-store`.
- Migrations live under root `migrations/` and are applied through store-owned migration code.
- Audit hash-chain writes are store-owned and exposed through CLI/SDK query surfaces.
- Retention/redaction jobs are store-owned but observable through CLI and evidence export.
- Capability JSON Schema belongs to TypeBox/source SDK layer; internal envelope/registry/audit models belong to Protobuf.

### Requirements to Structure Mapping

**FR Category Mapping:**

| Requirement Area | Primary Structure | Supporting Structure |
|---|---|---|
| FR1-FR10 Wire & Messaging | `crates/zornmesh-core`, `crates/zornmesh-rpc`, `crates/zornmesh-broker` | `fixtures/frames`, `conformance/protocol`, `crates/zornmesh-sdk`, `sdks/typescript` |
| FR11-FR14 Identity & Capabilities | `crates/zornmesh-core`, `crates/zornmesh-broker`, `crates/zornmesh-store` | `fixtures/a2a/agentcard-1.0.0`, `sdks/typescript/src/agent-card.ts`, `docs/agentcard-profile.md` |
| FR15-FR21 Daemon Lifecycle | `crates/zornmesh-daemon`, `crates/zornmesh-sdk/src/spawn.rs`, `crates/zornmesh-rpc` | `crates/zornmesh-cli/src/commands/daemon.rs`, `fixtures/cli/help/help-daemon.txt`, `conformance/security/no-default-tcp.json` |
| FR22-FR28 Persistence & Forensics | `crates/zornmesh-store`, `migrations/`, `crates/zornmesh-cli/src/commands/trace.rs`, `replay.rs`, `inspect.rs` | `conformance/persistence`, `docs/architecture/persistence.md` |
| FR29-FR32 Observability & Tracing | `crates/zornmesh-core`, `crates/zornmesh-rpc`, `test-infra/otel-collector.yaml` | `crates/zornmesh-cli/src/commands/tail.rs`, `trace.rs`, `conformance/observability` |
| FR33-FR35 Host Integration | `crates/zornmesh-rpc/src/mcp.rs`, `crates/zornmesh-cli/src/commands/stdio.rs` | `fixtures/mcp/2025-11-25`, `docs/mcp-compatibility.md` |
| FR36-FR40 Security & Trust | `crates/zornmesh-rpc/src/platform`, `crates/zornmesh-daemon`, `crates/zornmesh-core/errors.rs` | `conformance/security`, `docs/architecture/security.md`, `SECURITY.md` |
| FR41-FR44 Compliance & Audit | `crates/zornmesh-store/src/audit.rs`, `redaction.rs`, `retention.rs`, `crates/zornmesh-cli/src/commands/audit.rs` | `docs/compliance`, `fixtures/errors/registry.json` |
| FR45-FR48 Developer & Operator CLI | `crates/zornmesh-cli` | `fixtures/cli`, `conformance/cli`, `docs/cli.md` |
| FR49-FR60 Local Web Companion UI | `crates/zornmesh-cli/src/commands/ui.rs`, `crates/zornmesh-rpc/src/ui_gateway.rs`, `apps/local-ui` | `fixtures/ui`, `conformance/ui`, browser E2E fixtures, accessibility fixtures, external-request-blocking fixtures |
| FR61-FR62 SDK Parity & Per-call Context | `crates/zornmesh-sdk`, `sdks/typescript` | `conformance/protocol`, `fixtures/errors`, `fixtures/a2a`, `fixtures/mcp`, cross-SDK context propagation fixtures |

**Cross-Cutting Concerns:**

| Concern | Location |
|---|---|
| Error registry | `crates/zornmesh-core/src/errors.rs`, `fixtures/errors/registry.json`, `docs/jsonrpc-error-codes.md` |
| ACK taxonomy | `crates/zornmesh-core/src/ack.rs`, `docs/architecture/ack-taxonomy.md`, `conformance/protocol/ack-taxonomy.json` |
| Frame format | `crates/zornmesh-rpc/src/frame.rs`, `fixtures/frames`, `docs/architecture/framing.md` |
| Version constants | `crates/zornmesh-core/src/versions.rs` |
| CLI output contract | `crates/zornmesh-cli/src/output.rs`, `fixtures/cli`, `conformance/cli/output-contracts.json` |
| Config precedence | `crates/zornmesh-daemon/src/config.rs`, `docs/configuration.md` |
| Release integrity | `.github/workflows/release.yml`, `xtask/src/release.rs`, `docs/release-integrity.md` |
| Forbidden patterns | `xtask/src/forbidden_patterns.rs`, `conformance/meta/forbidden-patterns.json` |

### Integration Points

**Internal Communication:**

1. Application SDK calls `connect()`.
2. SDK resolves local socket path and attempts UDS/named-pipe connect.
3. SDK auto-spawns daemon when absent unless explicitly disabled.
4. SDK and daemon perform initialize/version negotiation.
5. SDK sends framed JSON-RPC/envelope payloads through `zornmesh-rpc`.
6. Daemon hands accepted envelopes to `zornmesh-broker`.
7. Broker routes, applies delivery semantics, and asks `zornmesh-store` for durable state changes.
8. Store commits before durable ACK.
9. CLI and SDK query trace/replay/audit data through client boundaries, not direct SQLite access.

**External Integrations:**

- MCP hosts via stdio compatibility in `zornmesh-rpc::mcp` and `zornmesh-cli stdio`.
- A2A AgentCard-compatible identity metadata through registry/capability paths.
- OpenTelemetry through test-infra collector assets and OTLP export.
- Prometheus through explicit opt-in loopback metrics gateway only.
- Sigstore/CycloneDX through release workflow and `xtask release` validation.

**Data Flow:**

```text
SDK/CLI/MCP host
  -> zornmesh-rpc frame parser
  -> JSON-RPC / initialize validation
  -> zornmesh-core envelope/domain validation
  -> capability JSON Schema validation
  -> zornmesh-broker route/delivery state
  -> zornmesh-store SQLite transaction
  -> durable ACK / NACK
  -> trace + audit + CLI/SDK observable output
```

**Trust Flow:**

```text
local process
  -> OS socket credential check
  -> socket permission validation
  -> daemon privilege check
  -> AgentCard identity registration
  -> capability policy evaluation
  -> validated message dispatch
```

### File Organization Patterns

**Configuration Files:**

- Root `Cargo.toml` defines workspace membership and shared dependency policy.
- `rust-toolchain.toml` pins stable toolchain policy.
- `.bun-version` pins Bun for the TypeScript SDK workspace.
- `justfile` is the human command entrypoint.
- `xtask` is the automation entrypoint for generation, fixtures, release checks, and conformance.
- Local product config examples live under `docs/configuration.md`; runtime config parsing lives in `zornmesh-daemon`.

**Source Organization:**

- Domain primitives live lower in the dependency graph.
- Runtime orchestration lives at daemon/CLI edges.
- Protocol parsing lives in `zornmesh-rpc`.
- Business delivery behavior lives in `zornmesh-broker`.
- Persistence lives in `zornmesh-store`.
- SDKs never depend on daemon internals.
- CLI never bypasses SDK/RPC boundaries.

**Test Organization:**

- Unit tests remain close to code.
- Integration tests live in crate `tests/` directories.
- Cross-language conformance lives in `conformance/` and `fixtures/`.
- Fault-injection scenarios live in `test-infra/fault-injection/`.
- CLI golden tests consume `fixtures/cli/`.
- Binary frame tests consume `fixtures/frames/`.
- MCP/A2A tests consume versioned fixture directories.

**Asset Organization:**

- Local UI source lives under `apps/local-ui`.
- Local UI built assets are bundled into the release artifact and served only by the explicit loopback UI session created by `zornmesh ui`.
- Runtime external browser requests are forbidden for UI assets: no CDN scripts, remote fonts, analytics, images, or remote config.
- Browser E2E, accessibility, responsive, reconnect/backfill, and external-request-blocking fixtures are product assets and must remain reviewed, versioned, and deterministic.
- Generated code is not a source-of-truth asset; schemas, fixtures, and UI/API contract definitions are.

### Development Workflow Integration

**Development Server Structure:**

- No always-on development server is required for core use.
- `zornmesh daemon` can run in foreground for local development.
- SDK tests may spawn isolated daemon instances through test harness helpers.
- Metrics gateway is explicit and opt-in in all environments.

**Build Process Structure:**

- `just` dispatches common developer commands.
- `cargo xtask` coordinates codegen, fixtures, conformance, and release preflight.
- Cargo builds all Rust crates.
- Bun builds/tests `sdks/typescript`.
- CI validates forbidden patterns and conformance fixtures.

**Deployment Structure:**

- v0.1 distribution is signed `zornmesh` binaries plus SDK packages and documentation.
- Release workflow produces SBOMs and signatures.
- Docker is not part of core runtime deployment.
- Runtime state lives in OS-appropriate local user data/runtime locations, not inside the repo tree.
- SQLite database and sockets are runtime artifacts, never committed fixtures except synthetic test databases explicitly placed under fixtures/conformance when needed.

### Boundary Anti-Patterns

- `zornmesh-cli` opening SQLite directly.
- `zornmesh-sdk` depending on `zornmesh-daemon`.
- `zornmesh-store` importing broker routing logic.
- `zornmesh-broker` parsing raw frame bytes.
- Any crate other than `zornmesh-rpc` inventing frame types.
- Any component opening TCP by default.
- TypeScript SDK adding npm/pnpm/yarn lockfiles.
- Conformance fixtures generated silently from current implementation output.
- Docs using `zorn`, `zorn-mesh`, or `zorn-meshd` as public commands.
- SQLite migrations hidden inside crate source instead of root `migrations/`.
- Python package code added before v0.2 scope is approved.

## Architecture Validation Results

### Coherence Validation

**Decision Compatibility: PASS**

The architectural decisions are mutually compatible:

- The selected custom Rust/Bun monorepo matches the local-first daemon/SDK product shape.
- Rust 2024, Tokio-only async, `sqlx` SQLite, Bun-only TypeScript tooling, and shared conformance fixtures are aligned.
- The no-default-TCP decision is compatible with local IPC, opt-in loopback metrics, and disabled-by-default OTLP export.
- The gRPC decision is coherent: no public agent-facing gRPC data plane, while OTLP/gRPC remains allowed for observability export.
- The MCP `2025-11-25` target and A2A `1.0.0` AgentCard target are pinned instead of floating.
- The TypeBox/Protobuf split is coherent because each layer owns a different schema domain.
- ACK taxonomy resolves ambiguity between protocol control frames, durable persistence, and consumer delivery.

**Pattern Consistency: PASS**

The implementation patterns support the decisions:

- `zornmesh` naming is consistently reflected across command names, release artifacts, CLI fixtures, and project structure.
- JSON `snake_case`, stable error codes, NDJSON streaming, and CLI stdout/stderr separation support the CLI contract.
- Fixture directories map directly to high-risk surfaces: frames, MCP, AgentCard, errors, CLI, persistence, security, and observability.
- Crate ownership rules prevent circular or leaky boundaries.
- Error, validation, daemon lifecycle, and persistence patterns are explicit enough to prevent incompatible agent-generated implementations.

**Structure Alignment: PASS**

The project structure supports the architecture:

- Every selected crate has a clear boundary and implementation reason.
- `zornmesh-store` owns SQLite; CLI and SDKs do not bypass it.
- `zornmesh-rpc` owns frame parsing, MCP, local transport, and optional metrics gateway boundaries.
- `zornmesh-broker` owns routing/delivery behavior without owning persistence schema.
- `xtask`, `conformance/`, and `fixtures/` provide enforcement paths for consistency.
- Python is represented only as a README boundary marker, preserving the v0.2 deferral.

### Requirements Coverage Validation

**Feature Coverage: PASS**

The 10 PRD functional-requirement groups are architecturally supported:

| Requirement Area | Validation Result |
|---|---|
| FR1-FR10 Wire & Messaging | Covered by core/rpc/broker, frame fixtures, ACK taxonomy, delivery state, streaming/backpressure decisions |
| FR11-FR14 Identity & Capabilities | Covered by A2A AgentCard profile, capability schema boundary, registry/store ownership |
| FR15-FR21 Daemon Lifecycle | Covered by daemon, SDK spawn/rendezvous, socket ownership, CLI daemon command, lifecycle patterns |
| FR22-FR28 Persistence & Forensics | Covered by SQLite/sqlx, migrations, audit hash chain, retention/redaction, trace/replay CLI boundaries |
| FR29-FR32 Observability & Tracing | Covered by W3C tracecontext, OTel, trace/tail commands, opt-in metrics, cardinality rules |
| FR33-FR35 Host Integration | Covered by MCP `2025-11-25` stdio adapter and internal/external protocol boundary |
| FR36-FR40 Security & Trust | Covered by UDS credential checks, socket permissions, high-privilege default deny, redaction, release integrity |
| FR41-FR44 Compliance & Audit | Covered by audit store, evidence export docs, redaction semantics, compliance documentation paths |
| FR45-FR48 Developer & Operator CLI | Covered by CLI crate, fixtures, output contracts, exit codes, completions-ready structure |
| FR49-FR60 Local Web Companion UI | Covered by `zornmesh ui`, daemon-owned UI API/live gateway, bundled local UI assets, browser E2E fixtures, accessibility/responsive fixtures, and audit/redaction parity |
| FR61-FR62 SDK Parity & Context Propagation | Covered by Rust SDK, Bun TypeScript SDK, shared conformance, idempotency/trace/timeout patterns |

**Functional Requirements Coverage: PASS**

All FR categories have explicit architectural ownership. No functional area remains unassigned.

**Non-Functional Requirements Coverage: PASS WITH DOWNSTREAM DETAIL NEEDED**

The architecture addresses all NFR groups:

- **Performance:** local IPC, bounded frames, streaming chunks, writer/reader split, backpressure, no external runtime dependency.
- **Security:** local-only default, socket trust, root/admin refusal, redaction, default-deny high-privilege capabilities, no default TCP.
- **Reliability:** single-daemon ownership, WAL, ACK-after-commit, migrations, crash recovery hooks, graceful drain.
- **Scalability:** single-machine scope, caps, parser ceilings, conformance fixtures, metric cardinality limits.
- **Compatibility:** Linux/macOS scope, MCP version pin, A2A profile pin, Rust/TypeScript SDK split, Python deferral.
- **Observability:** W3C tracecontext, OTel, trace/tail CLI, opt-in metrics.
- **Maintainability:** crate ownership, pattern enforcement, fixture corpus, CI/xtask gates.
- **Compliance/Auditability:** audit hash chain, evidence export docs, retention/redaction ownership, SBOM/signature paths.

Downstream stories must still turn these into concrete acceptance criteria and tests.

### Implementation Readiness Validation

**Decision Completeness: PASS**

Critical implementation-blocking decisions are complete:

- binary naming
- wire framing
- ACK taxonomy
- gRPC scope
- MCP version
- A2A AgentCard version
- TypeScript runtime/support boundary
- schema ownership
- SQLite persistence model
- metrics exposure
- CLI output contracts
- fixture/conformance ownership

**Structure Completeness: PASS**

The architecture defines:

- root files
- Rust crate layout
- SDK layout
- migration layout
- fixture layout
- conformance layout
- test-infra layout
- docs layout
- CI/release workflow locations
- component boundaries
- integration/data/trust flows

**Pattern Completeness: PASS**

Patterns cover:

- product/artifact naming
- database naming
- protocol/API naming
- Rust/TypeScript code naming
- test placement
- crate ownership
- JSON and error formats
- event/log/observability naming
- config precedence
- validation order
- lifecycle states
- persistence process rules
- enforcement and anti-patterns

### Gap Analysis Results

**Critical Gaps: None in architecture content**

No unresolved architecture decision blocks epic/story generation.

**Important Downstream Gaps:**

1. **Epics/stories do not exist yet.** The project should run the epics/stories workflow after architecture completion.
2. **Implementation-readiness status remains not-ready until downstream artifacts exist.** The previous readiness report should be rerun after epics/stories are created.
3. **Source-doc contradictions should be normalized downstream.** Project-context examples still reference MCP `2025-03-26` and `zorn-meshd`; architecture now resolves those to MCP `2025-11-25` and public `zornmesh`.
4. **Fixture schemas need concrete definitions during implementation planning.** Architecture names fixture paths and ownership, but stories must define exact JSON schemas and golden contents.
5. **SQLite operational values need story-level ACs.** Busy timeout, checkpoint threshold, migration-lock behavior, and crash recovery cases are intentionally called out but not numerically specified here.
6. **TypeScript SDK runtime support is intentionally narrow.** Bun-only support is clear, but user-facing docs must make this explicit to avoid ecosystem confusion.

**Nice-to-Have Gaps:**

1. Add a visual dependency diagram after epics/stories are generated.
2. Add an ADR index if decision volume grows.
3. Add a fixture authoring guide before multiple agents begin writing fixtures.
4. Add a compatibility matrix page for MCP/A2A/SDK versions.

### Validation Issues Addressed

| Issue Found | Resolution |
|---|---|
| ACK ambiguity risk | Resolved with transport/durable/delivery ACK taxonomy |
| Public binary naming ambiguity | Resolved to `zornmesh` |
| gRPC contradiction | Resolved to OTLP-only in v0.1 |
| MCP version drift | Resolved to `2025-11-25` |
| A2A/AgentCard floating-version risk | Resolved to A2A `1.0.0` AgentCard profile |
| TypeBox vs Protobuf authority risk | Resolved with split schema ownership |
| Prometheus vs no-TCP contradiction | Resolved with opt-in loopback-only metrics gateway |
| CLI UX readiness gap | Addressed through CLI output contract, fixtures, and structure |
| Multi-agent implementation drift | Addressed through patterns, crate ownership, fixtures, and anti-patterns |

### Architecture Completeness Checklist

**Requirements Analysis**

- [x] Project context thoroughly analyzed
- [x] Scale and complexity assessed
- [x] Technical constraints identified
- [x] Cross-cutting concerns mapped
- [x] PRD/project-context conflicts identified and resolved or superseded

**Architectural Decisions**

- [x] Critical decisions documented with versions
- [x] Technology stack specified
- [x] Integration patterns defined
- [x] Performance constraints addressed architecturally
- [x] Security defaults addressed architecturally
- [x] Persistence and ACK semantics addressed
- [x] Observability surfaces addressed
- [x] CLI UX contract addressed

**Implementation Patterns**

- [x] Naming conventions established
- [x] Structure patterns defined
- [x] Communication patterns specified
- [x] Process patterns documented
- [x] Enforcement guidance defined
- [x] Anti-patterns documented

**Project Structure**

- [x] Complete directory structure defined
- [x] Component boundaries established
- [x] Integration points mapped
- [x] Requirements-to-structure mapping complete
- [x] Test/fixture/conformance locations defined
- [x] Documentation locations defined

### Architecture Readiness Assessment

**Architecture Status:** READY FOR IMPLEMENTATION READINESS RERUN AFTER THIS LOCAL UI AMENDMENT IS APPLIED

**Program Implementation Gate:** NOT READY until the updated architecture, epics dependency graph, and UX stale-note cleanup are applied and Implementation Readiness passes.

**Confidence Level:** High for architecture coherence; medium-high for implementation planning because the next workflow still needs to convert architecture into story-level acceptance criteria.

**Key Strengths:**

- Strong local-first boundary with no default network exposure.
- Clear protocol and persistence decisions.
- Explicit CLI UX contract.
- Explicit local UI scope, gateway boundary, session/security model, and bundled asset posture.
- Strong fixture/conformance discipline.
- Clear crate ownership and anti-patterns.
- Cross-language SDK parity supported through shared fixtures.
- Compliance/audit concerns included early rather than deferred.

**Areas for Future Enhancement:**

- Full public ADR index.
- Post-v0.1 Python SDK architecture expansion.
- Hosted/cloud dashboard, LAN/public console, accounts/teams, full chat workspace, and workflow editor architecture.
- Future public gRPC data-plane evaluation.
- More detailed fixture authoring and schema registry docs.

### Implementation Handoff

**AI Agent Guidelines:**

- Follow all architectural decisions exactly as documented.
- Use implementation patterns consistently across all components.
- Respect project structure and crate boundaries.
- Treat fixtures and conformance as executable requirements.
- Do not reopen gRPC, binary naming, MCP version, A2A profile, Bun runtime, or SQLite driver choices without a formal architecture change.
- Use this architecture as the source for epics/stories, then rerun implementation readiness.

**First Implementation Priority:**

The first story should create the custom `zornmesh` workspace scaffold and a thin protocol-neutral smoke path:

- Cargo workspace with Rust 2024 resolver `3`.
- Initial crates and boundaries.
- `zornmesh` CLI help output fixture.
- Bun TypeScript SDK package skeleton.
- Initial conformance/fixture directories.
- `xtask` or `just` validation entrypoint.
- No gRPC data-plane smoke path.
- No default TCP listener.
- No Python package implementation beyond optional README boundary.
