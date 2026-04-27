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
cargo run -p zornmesh-cli -- daemon --help
cargo run -p zornmesh-cli -- trace --help
```

The generated output is fixture-checked under `fixtures/cli/`.

## Local daemon rendezvous

`zornmesh daemon` starts the local Unix-domain socket daemon, prints a parseable readiness line (`zorn: state=ready socket=<path>`), and owns a private per-user socket. The daemon rejects elevated-privilege startup, unsafe socket ownership or permissions, active duplicate owners, and stale untrusted sockets with stable error codes.

Useful environment variables are documented in [`docs/env-vars.md`](docs/env-vars.md). The first lifecycle variables are `ZORN_SOCKET_PATH`, `ZORN_NO_AUTOSPAWN`, and `ZORN_SHUTDOWN_BUDGET_MS`.

## TypeScript SDK (Bun)

The TypeScript SDK lives under `sdks/typescript`, is managed by Bun, and exposes the first local `connect`, `publish`, and `subscribe` entrypoints using zornmesh naming.

```bash
cd sdks/typescript
bun install
bun test
```

```ts
import { connect } from "@zornmesh/sdk";

const mesh = await connect({ agentId: "agent.local/typescript" });
const subscription = await mesh.subscribe("mesh.trace.>");
const result = await mesh.publish({
  subject: "mesh.trace.created",
  payload: JSON.stringify({ trace_id: "trace-1" }),
});
const delivery = await subscription.recvDelivery(500);
```
