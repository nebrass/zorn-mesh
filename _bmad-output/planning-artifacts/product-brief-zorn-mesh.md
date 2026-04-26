---
title: "Product Brief: Zorn Mesh"
project_name: "zorn-mesh"
status: "complete"
created: "2026-04-26"
updated: "2026-04-26"
author: "Nebrass"
mode: "yolo"
source_inputs: ["_bmad-output/project-context.md", "Claude.pdf", "GeminiReport.pdf", "Perplexity.pdf"]
---

# Product Brief: Zorn Mesh

**A local-first agent IPC fabric — the SQLite of agent buses.**

## Executive Summary

Zorn Mesh is a single-binary local broker that lets coding agents on the same developer machine — Claude, Copilot, Gemini, custom Python and TypeScript agents — find each other, exchange typed messages, subscribe to topics, replay history, and be inspected. Securely and reliably. Without dragging in a network broker.

The 2026 agent-protocol landscape has settled on two standards: **MCP** for tool-calling between hosts and servers, and **A2A v0.3** for opaque-agent peer RPC. Both assume one host talking to one server, or two well-known agents on a network. Neither solves the problem developers actually have today: *three to five agents running on my laptop right now, in different processes and languages, that need to coordinate.* Today they don't — they sit in isolated host applications with ad-hoc bridges. Zorn Mesh fills that gap and stays bilingual with MCP, so any existing MCP client can join the bus as if it were an MCP server.

The bet is simple: **MCP is the wire, NATS is the model, SQLite is the store, Rust is the runtime.** No new ideas — just the right ones, packaged for the developer machine.

## The Problem

A developer wiring multiple agents together today has zero infrastructure. Each agent host (Claude Desktop, Cursor, VS Code Copilot, the Gemini CLI) is an island. Agents in different processes or languages cannot share state, broadcast events, address each other peer-to-peer, or replay what happened five minutes ago. When a three-agent pipeline breaks at 2 a.m., there is nothing to look at — no message log, no trace, no audit trail.

The coping behavior is universal: ad-hoc HTTP servers on random ports, JSON files in `/tmp`, environment variables passing handles around, manual prompt-engineered orchestration. Every team building serious multi-agent workflows rebuilds the same plumbing badly. The result is brittle, opaque, and nobody trusts it.

The cost compounds with the number of agents and the velocity of the agent ecosystem. Every new MCP server, every new A2A peer, every new local LLM agent is another participant that cannot speak to the others without bespoke glue.

## The Solution

Zorn Mesh is a **library plus an auto-spawned daemon**. An agent calls `Mesh.connect()`. If the daemon is not running, the library spawns it (the `sccache` pattern). The agent registers, advertises capabilities, and immediately can publish to topics, request from peers, subscribe with wildcards, and stream chunks — all over JSON-RPC 2.0 with LSP-style framing on a Unix-domain socket.

The daemon — `zornmesh` — is one Rust binary, statically compiled, around 20 megabytes. Its only state is a single SQLite file at `~/.local/state/zornmesh/mesh.db`. Its only network surface is a Unix-domain socket guarded by `SO_PEERCRED` UID-match.

**Three primitives carry most of the value:**

1. **`mesh.publish` / `mesh.subscribe`** with NATS-style hierarchical topics (`agent.copilot.suggestion.>`) and durable subscriptions, so agents broadcast and consume without coordinating sender and receiver.
2. **`mesh.request` / `mesh.fetch` / `mesh.ack`** for at-least-once peer RPC with leases and pull-based delivery — the pattern proven by SQS and JetStream, sized for one machine.
3. **`zornmesh trace <correlation_id>`** — paste any correlation ID at the terminal and see every envelope it touched, in order, with timing. Jaeger without leaving the shell.

For MCP interop, the daemon also speaks plain stdio JSON-RPC: any MCP client can run `zornmesh stdio --as-agent <id>` and join the bus as if Zorn Mesh were an MCP server. Existing tools work unchanged.

## What Makes This Different

- **Local-first by architecture, not by deployment toggle.** No broker to install, no port to allocate, no cloud account, no Docker, no Go runtime. The daemon auto-spawns on first connect.
- **MCP-superset wire, not MCP-replacement.** The framing, lifecycle, and method namespaces are byte-compatible with MCP 2025-11-25; Zorn-specific methods live under the reserved `mesh/*` namespace and never collide. An MCP client that only cares about tools sees Zorn Mesh as a regular MCP server with a small set of mesh-management tools.
- **Symmetric capability model.** Unlike MCP's hub-and-spoke, both ends of every connection advertise both consumer and provider capabilities at handshake. This is what makes peer-to-peer agent comms feel native rather than tunneled.
- **Inspectable by design.** SQLite store, Unix-domain socket, JSON wire, structured tracing via OpenTelemetry. `sqlite3`, `socat`, and `zornmesh tail` are first-class debug surfaces. The 2 a.m. on-call story is `sqlite3 mesh.db 'SELECT * FROM messages WHERE stream=? ORDER BY offset DESC LIMIT 50'` — a superpower no key-value store offers.
- **Honest reliability claims.** At-least-once delivery with mandatory idempotency keys, full-jitter exponential backoff, dead-letter queue, lease-based pull delivery. Exactly-once is explicitly rejected — Jepsen 2026 demonstrated it is a marketing claim even NATS JetStream cannot keep, and lying about it erodes developer trust.
- **A 22 MB Rust binary** with the maturity of `tokio` + `tower` + `axum` + `tracing` + `sqlx`-style ecosystems behind it. Memory footprint of 5–15 MB RSS — fits alongside VS Code, Slack, Docker, and three browsers without notice.

## Who This Serves

**Primary:** Developers building multi-agent systems on their laptop today. The Cursor user wiring custom MCP servers. The Claude Desktop user orchestrating a Python research agent and a TypeScript code-review agent. The Open Code user running Gemini and Copilot side-by-side. The Rust developer prototyping an agent runtime who needs IPC that does not require Kubernetes.

What success looks like for them: open a terminal, write twelve lines of code, see three agents communicate and a `zornmesh trace` reconstruct the conversation. Ten minutes from `cargo install` to first message.

**Secondary (post-MVP):** Platform engineers at mid-size organizations standing up internal agent platforms. They need a local-first prototype substrate that mirrors the eventual multi-host deployment shape, so the same envelope and capability model survives the transition. The v0.5 A2A gateway and the v1.0 federation layer are the upgrade paths.

**Explicitly not the audience:** teams that already run NATS or Kafka for cross-machine messaging. Zorn Mesh is single-machine by design and refuses scope creep into general service mesh.

## Success Criteria

- **v0.1 (MVP):** the three-agent example (`examples/three_agents.rs`) compiles and runs end-to-end in under 10 minutes for someone who has never seen the tool. `zornmesh trace` produces a complete timeline. Zero CVEs at launch. Linux + macOS only.
- **v0.5:** 1,000 weekly active developers; at least one major agent runtime ships a Zorn Mesh adapter in its default configuration; the A2A bridge has at least one production deployment outside the core team.
- **v1.0:** the MCP-superset wire is recognized as a deployable profile in at least one independent agent framework; federation across machines via TLS-tunneled gRPC is demonstrated.

The North Star metric is **time-to-first-coordinated-message** — the wall-clock time from `cargo install zornmesh` to a developer seeing two agents exchange envelopes through the bus. Target: under 600 seconds. Everything else flows from that number.

## Scope

**v0.1 (in scope):** agent registration with capabilities, presence and heartbeats, request/reply with correlation and cancellation, fire-and-forget events, topic pub/sub with `*` and `>` wildcards, streaming chunks, append-only message log with offset-based replay, OS-level trust (UID match + socket ACL), structured OpenTelemetry tracing, the `zornmesh` CLI (`tail`, `trace`, `agents`, `inspect`, `doctor`, `replay`, `stdio`). Linux and macOS. Rust SDK and TypeScript SDK at parity.

**v0.1 (explicitly deferred):** per-agent cryptographic identity, signed envelopes, capability tokens, web dashboard, A2A bridge, AGNTCY/SLIM bridge, Windows named-pipe support (v0.2), Python SDK (v0.2), multi-host federation, any form of replication.

**The single biggest scope risk is creep into orchestration.** Zorn Mesh routes messages; it does not plan, retry tasks at the application layer, or own agent state. The bus stays dumb on purpose, the way Postfix and NATS stay dumb.

## Vision

Zorn Mesh becomes the default IPC fabric agents reach for when they need to coordinate on a single machine without standing up a broker — the way developers reach for SQLite when they need a database without standing up a server. In two to three years, every major agent runtime ships a Zorn Mesh adapter out of the box, and the `mesh/*` method namespace is recognized alongside `tools/*` and `resources/*` as part of the protocol vocabulary developers learn first. The single-binary, single-file, single-socket model proves that local-first agent infrastructure does not need to compromise on observability, reliability, or interop — and the federation layer extends the same model to multi-host deployments without architectural rework.

## Technical Approach (high-level)

A Rust core on `tokio` 1.47 LTS using a `tower`-service architecture. JSON-RPC 2.0 with LSP-style `Content-Length` framing as the canonical wire. Protobuf in `proto/zorn/mesh/v0/` as the canonical schema, with `@bufbuild/protobuf` for the TypeScript SDK and `protobuf` runtime + Pydantic v2 for the Python SDK. `rusqlite` 0.32 in WAL mode for persistence, single SQLite file. OpenTelemetry for tracing/metrics/logs. Cargo workspace monorepo with `cargo nextest` + `proptest` + `loom` + `turmoil` testing stack and `buf breaking` enforcing wire stability. Full technical specification in `_bmad-output/project-context.md`.

## Roadmap

| Release | When | Headline |
|---|---|---|
| **v0.1 MVP** | ~10 weeks from start | Linux + macOS, Rust + TS SDKs, full feature set above |
| **v0.2** | ~6 weeks after v0.1 | Ed25519 signed envelopes, Windows named-pipe support, Python SDK |
| **v0.5** | TBD | Capability tokens (biscuit-auth), web dashboard, A2A gateway |
| **v1.0** | TBD | Stable wire protocol with deprecation policy, multi-host federation |
