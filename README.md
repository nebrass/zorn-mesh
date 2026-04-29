# zornmesh

[![crates.io](https://img.shields.io/crates/v/zornmesh.svg)](https://crates.io/crates/zornmesh)
[![GitHub Release](https://img.shields.io/github/v/release/nebrass/zorn-mesh)](https://github.com/nebrass/zorn-mesh/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

`zornmesh` is a local-first agent mesh and Model Context Protocol (MCP) stdio bridge. A per-user Rust daemon owns a private Unix-domain socket; an MCP-compatible host (Claude Desktop, Claude Code, Cursor, Windsurf, VS Code) connects to it via `zornmesh stdio --as-agent <id>`. No SaaS, no network listeners — every byte stays on loopback. macOS and Linux only.

## Install

| Channel | Command | When to use |
|---|---|---|
| Homebrew | `brew install nebrass/tap/zornmesh` | macOS / Linux, fastest path |
| cargo-binstall | `cargo binstall zornmesh` | Rust users, skips compilation |
| crates.io | `cargo install zornmesh` | Universal Rust fallback |
| GitHub Release | [Download a tarball](https://github.com/nebrass/zorn-mesh/releases/latest) | No package manager available |
| Docker | `docker run --rm -i ghcr.io/nebrass/zornmesh:latest stdio --as-agent default` | Container isolation |

Long-form install matrix and host-config table: [`docs/install.md`](docs/install.md).

Windows is not supported in v0.1 — the daemon is built around Unix-domain sockets.

## Wire it into your MCP host

Standard config (Claude Desktop, Claude Code, Cursor, Windsurf — `mcpServers` key):

```json
{
  "mcpServers": {
    "zornmesh": {
      "command": "zornmesh",
      "args": ["stdio", "--as-agent", "default"]
    }
  }
}
```

For VS Code, replace `mcpServers` with `servers`. For Claude Code, the CLI shortcut writes the JSON for you:

```bash
claude mcp add zornmesh -- zornmesh stdio --as-agent default
```

`zornmesh stdio` autospawns the per-user daemon on first connect (set `ZORN_NO_AUTOSPAWN=1` to opt out, override the readiness window with `ZORN_AUTOSPAWN_TIMEOUT_MS`). For a supervised, login-persistent daemon, run `zornmesh service install` once and follow the printed activation hint (`launchctl bootstrap` on macOS, `systemctl --user enable` on Linux).

## Verify

```bash
zornmesh --version            # zornmesh 0.1.0
zornmesh service status       # installed=false reachable=false …
zornmesh stdio --help
```

## Development

Required toolchain: Rust stable (with `rustfmt` + `clippy`), `bun`, `just`. `cargo xtask` is the orchestrator; `just` targets are thin wrappers around it.

```bash
just check        # cargo check --workspace --all-targets
just test         # workspace + bun test (sdks/typescript + apps/local-ui)
just lint         # cargo fmt --check + cargo clippy -D warnings
just docs         # cargo doc --workspace --no-deps
just conformance  # envelope_round_trip + golden_help + daemon_help

cargo run -p zornmesh -- --help          # smoke check; output is fixture-pinned in fixtures/cli/
```

Releases are cut by tagging `v*.*.*` and letting `.github/workflows/release.yml` fan out to GitHub Releases, the Homebrew tap, and GHCR. See [`RELEASE.md`](RELEASE.md) for the maintainer checklist.

## Architecture

| Crate | Role |
|---|---|
| [`zornmesh-core`](https://crates.io/crates/zornmesh-core) | Shared primitives |
| [`zornmesh-proto`](https://crates.io/crates/zornmesh-proto) | Wire protocol, envelope round-trip |
| [`zornmesh-store`](https://crates.io/crates/zornmesh-store) | Persistence, evidence, audit, retention |
| [`zornmesh-rpc`](https://crates.io/crates/zornmesh-rpc) | Local Unix-socket RPC layer |
| [`zornmesh-broker`](https://crates.io/crates/zornmesh-broker) | Subject-pattern pub/sub broker |
| [`zornmesh-daemon`](https://crates.io/crates/zornmesh-daemon) | Per-user daemon owning the socket |
| [`zornmesh-sdk`](https://crates.io/crates/zornmesh-sdk) | Rust SDK |
| [`zornmesh`](https://crates.io/crates/zornmesh) | CLI binary, MCP stdio bridge |

A TypeScript SDK lives under [`sdks/typescript`](sdks/typescript) and ships as `@zornmesh/sdk` (Bun-managed); a Bun-bundled React local UI ([`apps/local-ui`](apps/local-ui)) is offline-served by the daemon UI gateway on loopback only.

```ts
import { connect } from "@zornmesh/sdk";

const mesh = await connect({ agentId: "agent.local/typescript" });
const subscription = await mesh.subscribe("mesh.trace.>");
await mesh.publish({
  subject: "mesh.trace.created",
  payload: JSON.stringify({ trace_id: "trace-1" }),
});
const delivery = await subscription.recvDelivery(500);
```

## Daemon contract

`zornmesh daemon` prints exactly `zorn: state=ready socket=<path>` on readiness — this line is parsed by clients and stable. The daemon rejects elevated-privilege startup, unsafe socket ownership or permissions, active duplicate owners, and stale untrusted sockets with stable error codes.

Lifecycle environment variables (full list in [`docs/env-vars.md`](docs/env-vars.md)):

| Variable | Effect |
|---|---|
| `ZORN_SOCKET_PATH` | Override the resolved Unix-domain socket path |
| `ZORN_NO_AUTOSPAWN=1` | Disable autospawn; fail fast with a remediation hint |
| `ZORN_AUTOSPAWN_TIMEOUT_MS` | Readiness budget for autospawned daemons (default 5000) |
| `ZORN_SHUTDOWN_BUDGET_MS` | Drain budget on shutdown, capped at 60s (default 10s) |

## License

[MIT](LICENSE).
