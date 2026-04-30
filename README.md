# zornmesh

[![crates.io](https://img.shields.io/crates/v/zornmesh.svg)](https://crates.io/crates/zornmesh)
[![GitHub Release](https://img.shields.io/github/v/release/nebrass/zorn-mesh)](https://github.com/nebrass/zorn-mesh/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

`zornmesh` is a **local-first message bus between coding agents**, with a Model Context Protocol (MCP) stdio bridge so any MCP-compatible host (Claude Code, OpenCode, Copilot CLI, Gemini CLI, Cursor, Windsurf, VS Code) joins as a first-class participant. A per-user Rust daemon owns a private Unix-domain socket. No SaaS, no network listeners — every byte stays on loopback. macOS and Linux only.

The v0.2 release ships the **multi-agent debate substrate**: one driver (a coding agent acting on a user's prompt) broadcasts a plan; worker daemons running for each registered coding-agent CLI invoke their underlying tool in non-interactive mode and respond with critiques; the orchestrator synthesizes a consensus that explicitly preserves dissent. See "Multi-agent debate" below.

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
zornmesh --version            # zornmesh 0.1.2
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

cargo run -p zornmesh -- --help          # smoke check; output is fixture-pinned in crates/zornmesh-cli/fixtures/
```

Releases are cut by tagging `v*.*.*` and letting `.github/workflows/release.yml` fan out to GitHub Releases, the Homebrew tap, and GHCR. See [`RELEASE.md`](RELEASE.md) for the maintainer checklist.

## Architecture

`zornmesh` is published as a single crate. Internal modules under `crates/zornmesh-cli/src/` separate concerns at the source level:

| Module | Role |
|---|---|
| `core` | Shared primitives — `Envelope`, `ErrorCategory`, `CoordinationOutcome` |
| `proto` | Wire protocol, envelope round-trip |
| `store` | Persistence — evidence, audit, retention |
| `rpc` | Local Unix-socket trust + connect |
| `broker` | Subject-pattern pub/sub, capability policy, the MCP `StdioBridge` |
| `daemon` | Per-user daemon owning the socket |
| `sdk` | Rust SDK surface (`Mesh::connect`/`publish`/`subscribe`); not a separate published crate yet |

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

## Multi-agent debate

The v0.2 use case: a coding agent (the *driver*) announces what it intends to do; the other coding agents on the mesh (*workers*) critique the plan; the driver synthesizes the response and proceeds with an improved plan.

### Run a worker daemon per coding-agent CLI

One persistent process per platform — these subscribe to `debate.*.plan` and shell out to the underlying CLI in non-interactive mode when a plan arrives. Run each in its own `tmux` window or as a `launchd` / `systemd` user unit:

```bash
zornmesh worker --platform claude     # spawns `claude --print` on each plan
zornmesh worker --platform copilot    # spawns `copilot -p`
zornmesh worker --platform gemini     # spawns `gemini --print`
zornmesh worker --platform opencode   # spawns `opencode run`
```

Each worker registers as `agent.worker.<platform>` in the mesh, so the audit trail attributes every critique back to its source.

### Start a debate from the CLI

```bash
zornmesh debate run "Refactor payment.rs to add idempotency keys" \
  --repo $(pwd) \
  --timeout 30 \
  --quorum 2
```

The originator publishes a plan envelope to `debate.<id>.plan`, blocks until `quorum` critiques arrive (or the timeout fires), then prints a synthesized consensus that **explicitly preserves dissent points** — disagreements are surfaced, not averaged away.

### Start a debate from inside a coding-agent host

Until v0.3 ships a bespoke `zornmesh.debate_plan` MCP tool, drivers in Claude Code / Copilot CLI / Gemini CLI / OpenCode invoke `zornmesh debate run` via their host's bash tool. From any of those hosts, the prompt:

> Run `zornmesh debate run "..." --output json --timeout 30` and synthesize the consensus into your final plan.

works end-to-end as long as the worker daemons are running.

### Subject taxonomy

Stable as of v0.2 (`zornmesh.debate.v1`):

| Subject | Producer | Consumer |
|---|---|---|
| `debate.<id>.plan` | originator (driver) | workers |
| `debate.<id>.critique.<agent>` | workers | orchestrator |
| `debate.<id>.consensus` | orchestrator | originator + audit |

The orchestrator is in-process for v0.2 (lives inside the `zornmesh debate run` invocation). v0.3+ will move it behind a registered broker capability so the SDK and a future `zornmesh.debate_plan` MCP tool can drive it without spawning a CLI.

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
