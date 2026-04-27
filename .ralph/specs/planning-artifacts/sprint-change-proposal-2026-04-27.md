---
project: zorn-mesh
date: '2026-04-27'
status: approved
scope: minor
trigger: implementation-readiness-rerun-2026-04-27-09-15
artifactsModified:
  - _bmad-output/planning-artifacts/epics.md
artifactsUnchanged:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/architecture.md
  - _bmad-output/planning-artifacts/ux-design-specification.md
supersedes: 'sprint-change-proposal-2026-04-27.md@08:57 (pre-architecture-amendment run)'
---

# Sprint Change Proposal — 2026-04-27 (re-run)

**Author:** Bob (Scrum Master) · **Mode:** Batch · **Status:** Approved by user 2026-04-27.

## 1. Issue Summary

The 09:15 re-run of `/bmad-check-implementation-readiness` returned `NEEDS_WORK` with one major issue and one non-blocking warning, both anchored in Epic 6 / Story 6.1 of `epics.md`:

- **Major:** Epic 6 dependency gate (epics.md:2311) and Story 6.1's first acceptance criterion (epics.md:2325-2329) still demand creating/updating an architecture amendment that is already in place (architecture.md:90, 485, 692-onward; confirmed by ux-design-specification.md:51). A Phase 4 implementation agent reading Story 6.1 today would either redo resolved planning work or be blocked by an unsatisfiable "Given" precondition.
- **Warning:** Architecture says "Bun-managed React/Next.js" while UX (ux:315) and epics (epics.md:645, 2333) say "Bun-managed React" — must be pinned consistent with NFR-S8, NFR-S10, NFR-S11, and NFR-C7 before implementation begins.

## 2. Impact Analysis

| Surface | Impact |
| --- | --- |
| PRD | None — already supersedes earlier no-GUI text. |
| Architecture | None — amendment already present; framework-wording pin captured downstream in Story 6.1. |
| UX | None — already aligned. |
| Epics → Epic 6 dependency gate | Reframe gate prose around "verifies and references" plus framework-wording pin. |
| Epics → Story 6.1 | Retitle, retune user story, replace AC #1, tighten AC #2 to forbid Node-served / Next.js-runtime / remote-asset patterns. ACs #3–#6 unchanged. |
| Other epics and stories | None. |
| Code / CI | None. Phase 4 has not started. |

**Scope classification:** **Minor.** Single artifact, story-level edit, no FR coverage change, no dependency graph change.

## 3. Recommended Approach

Direct adjustment to Story 6.1 only. No rollback. No MVP descope. Effort ≈ 10 minutes editing plus one re-run of `/bmad-check-implementation-readiness` to confirm `READY`.

## 4. Detailed Change Proposals

### Change 1 — Epic 6 dependency gate (epics.md ~line 2311) [APPLIED]

OLD: gate demanded Story 6.1 "creates or updates the architecture artifact with an explicit v0.1 local UI amendment/ADR that supersedes stale no-GUI/frontend/static-asset statements …"

NEW: gate now requires Story 6.1 to (a) verify and reference — by section — the existing architecture amendment, (b) pin the local UI framework wording so no Node-served runtime, hosted serving model, or remote-asset dependency can enter v0.1, and (c) scaffold the app shell, shared UI/API taxonomies, fixture baseline, and scope-boundary checks against that referenced architecture.

### Change 2 — Story 6.1 title (epics.md ~line 2313) [APPLIED]

OLD: `Resolve UI Architecture Supersession and Scaffold Local Web App Shell`

NEW: `Verify Local UI Architecture, Pin Framework Wording, and Scaffold Local Web App Shell`

### Change 3 — Story 6.1 user story (epics.md ~lines 2315-2317) [APPLIED]

NEW user story states the developer wants verification, framework-wording pin, and shell scaffold so v0.1 cannot silently introduce a Node-served runtime, hosted serving model, or remote-asset dependency.

### Change 4 — Story 6.1 Acceptance Criterion #1 (epics.md ~lines 2325-2329) [APPLIED]

GIVEN flips to "the architecture artifact already contains the v0.1 local UI amendment." THEN clauses now require: (a) cite the existing amendment by section reference, (b) pin the framework wording explicitly to "Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only" with explicit rejection of Node-served runtime / hosted serving / Next.js server features / remote browser assets, and (c) record the same out-of-scope items as before, citing NFR-S8, NFR-S10, NFR-S11, NFR-C7.

### Change 5 — Story 6.1 Acceptance Criterion #2 (epics.md ~lines 2331-2334) [APPLIED]

THEN now requires the Bun-managed React app shell to produce statically bundled assets served only by the daemon UI gateway on loopback, with explicit rejection of Node-served runtime, Next.js server features, remote-asset dependencies, and external runtime services.

### Changes 6–9 — Story 6.1 Acceptance Criteria #3, #4, #5, #6

No changes. Tokens, shared state language, component fixtures, and scope-boundary surface ACs remain as authored.

## 5. Implementation Handoff

- **Scope:** Minor.
- **Edits applied directly to:** `_bmad-output/planning-artifacts/epics.md`.
- **Verification step:** re-run `/bmad-check-implementation-readiness`. Expected outcome `READY` (1 major and 1 warning closed; 62/62 FR coverage retained).
- **No PRD, architecture, or UX edits required.**
- **Stale artifact:** the 08:57 sprint-change-proposal predates today's epics+architecture edits and is superseded by this proposal.

## 6. Approval and Routing

Approved by user on 2026-04-27 (this turn). Edits applied. Direct hand-off back to Implementation Readiness for verification.

**Author:** Bob, Scrum Master.
