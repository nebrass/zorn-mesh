# Local UI Architecture Amendment (Story 6.1 verification)

This file is the implementation-side anchor for Story 6.1. It cites the existing
v0.1 local UI architecture amendment by section reference, pins the framework
wording for v0.1, and records the scope-boundary checks that future Epic 6
stories (6.2-6.9) must obey.

## 1. References to the existing v0.1 amendment

The architecture artifact at
[`.ralph/specs/planning-artifacts/architecture.md`](../../.ralph/specs/planning-artifacts/architecture.md)
already contains the v0.1 local UI amendment. Story 6.1 binds Epic 6 planning
to the following specific sections:

| Section reference                                    | Anchor in `architecture.md`     |
|------------------------------------------------------|----------------------------------|
| Architecture supersession note (no-GUI override)     | `architecture.md` line 90        |
| Local UI scope decision (v0.1 hosted/LAN exclusions) | `architecture.md` line 485       |
| Local UI process/trust boundary                      | `architecture.md` line 486       |
| Local UI security and asset posture                  | `architecture.md` line 488       |
| Local web companion UI (Frontend Architecture)       | `architecture.md` lines 690-701  |
| Capability-area mapping (FR49-FR60 / UI NFRs)        | `architecture.md` lines 125-126  |
| Repo layout for local UI app                         | `architecture.md` line 346       |

These sections supersede earlier no-GUI / no-frontend / no-static-asset text in
the PRD and product brief for v0.1 only.

## 2. Pinned framework wording for v0.1

> v0.1 local UI is a **Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only**.

This wording is exact and load-bearing. Any change to the framework wording for
v0.1 must update this anchor first; tests in
[`crates/zornmesh-cli/tests/local_ui_scope.rs`](../../crates/zornmesh-cli/tests/local_ui_scope.rs)
fail closed if the string disappears or is contradicted.

For v0.1, the local UI MUST NOT introduce:

- Node-served runtime
- Hosted serving model (cloud, SaaS, multi-tenant)
- Next.js server features (SSR, route handlers, middleware, edge runtime)
- Remote browser assets (CDN scripts, remote fonts, analytics, telemetry, remote config)
- External runtime services for the browser (no third-party APIs at runtime)

## 3. Scope boundaries for v0.1

In scope (the surfaces Stories 6.2-6.9 may build):

- Observe (live agent roster, daemon health, trust posture)
- Inspect (trace timeline, focused trace reader, event detail)
- Reconnect / backfill (recover daemon-sequence ordering after disconnect)
- Safe direct send (single-recipient validated send with audit lineage)
- Safe broadcast (multi-recipient send with explicit confirmation + drift checks)
- Outcome review (per-recipient outcome list, audit linkage)
- CLI handoff commands (copy-ready terminal commands with safe quoting)

Out of scope for v0.1, traceable to the cited NFRs:

| Out-of-scope surface                  | NFR anchor |
|---------------------------------------|------------|
| Hosted / cloud dashboard              | NFR-S8     |
| LAN / public console                  | NFR-S10    |
| Accounts / teams / multi-tenant       | NFR-S11    |
| Full chat workspace                   | NFR-C7     |
| Workflow editor                       | NFR-C7     |
| Remote browser assets / CDN           | NFR-S10    |
| External runtime services             | NFR-S10    |

Out-of-scope surfaces must either be **absent** from the local UI shell or
**return an explicit out-of-scope error** when reached. They must never appear
silently as supported or "coming soon" surfaces.

## 4. Component fixture and taxonomy baseline

The fixture-driven baseline lives under
[`apps/local-ui/`](../../apps/local-ui). The shell scaffolds shared design
tokens, shared state taxonomies (agent_status, delivery_state,
trace_completeness, daemon_health, trust_posture), primitive component
wrappers, and deterministic fixture states (baseline, loading, error, disabled,
focus, reduced-motion). Future Stories 6.2-6.9 extend this baseline; they
cannot replace it without first updating this amendment.
