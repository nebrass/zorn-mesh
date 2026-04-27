# zornmesh

`zornmesh` is a local-first agent mesh scaffold. Story 1.1 establishes the buildable Rust workspace, Bun-managed TypeScript SDK boundary, CLI help fixtures, and conformance/test ownership directories.

## Required tools

- Rust stable with `rustfmt` and `clippy`
- Bun
- Just

## Workspace commands

```bash
just check
just test
just lint
just docs
just conformance
```

The Justfile delegates to explicit `cargo xtask <subcommand>` entrypoints. Missing required tools fail with a named error instead of silently succeeding.

## CLI smoke path

```bash
cargo run -p zornmesh-cli -- --help
cargo run -p zornmesh-cli -- trace --help
```

The generated output is fixture-checked under `fixtures/cli/`.
