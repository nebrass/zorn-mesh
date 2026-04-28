# `@zornmesh/local-ui`

Bun-managed React app shell for the v0.1 local UI. Statically bundled and
offline-served by the daemon UI gateway on loopback only.

## Build / test (intended)

```sh
cd apps/local-ui
bun install
bun run build
bun test
bun run test:quality
bun run quality-evidence
```

The full Bun toolchain is not required to verify Story 6.1: the structural
invariants (framework wording, scope manifest, taxonomies, tokens, fixture
matrix) are checked from Rust via
[`crates/zornmesh-cli/tests/local_ui_scope.rs`](../../crates/zornmesh-cli/tests/local_ui_scope.rs)
so CI does not need a Bun runtime to gate the architecture contract.

Story 6.9 adds the local UI quality gate:
`src/quality-gates.ts` emits stable accessibility, responsive, browser,
offline-asset, and critical-journey readiness evidence. `bun test` validates
that the emitted evidence matches
[`fixtures/ui/quality-readiness.json`](../../fixtures/ui/quality-readiness.json)
and fails explicitly if required accessibility or browser fixture tooling is
marked unavailable.

## Anchors

- Architecture amendment: [`docs/architecture/local-ui-amendment.md`](../../docs/architecture/local-ui-amendment.md)
- Existing v0.1 amendment: `.ralph/specs/planning-artifacts/architecture.md` (sections cited in the amendment file).
