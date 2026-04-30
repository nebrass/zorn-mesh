# zornmesh

[![crates.io](https://img.shields.io/crates/v/zornmesh.svg)](https://crates.io/crates/zornmesh)
[![GitHub Release](https://img.shields.io/github/v/release/nebrass/zorn-mesh)](https://github.com/nebrass/zorn-mesh/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

`zornmesh` is a **multi-agent debate tool for local coding agents**. One CLI command (or one MCP tool call) parallel-spawns each installed coding-agent CLI — Claude Code, GitHub Copilot CLI, Gemini CLI, OpenCode — in non-interactive mode, asks each to critique a plan, and returns the aggregated responses. Every byte stays on the local machine; no SaaS, no network listeners, macOS and Linux only.

v0.3 deliberately ships **without** a daemon, broker, persistent workers, or pub/sub of any kind. The substrate from v0.1/v0.2 was over-engineered for the actual use case (everything runs on one laptop in one second-scale interaction). A debate is now one tool call: spawn N subprocesses in parallel, capture stdout with per-platform timeouts and bounded memory, append a JSONL audit record, return.

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

One CLI command runs the full debate. No background processes, no daemons, no service install — just `brew install` + the MCP config you already added per host.

```bash
zornmesh debate run "Refactor payment.rs to add idempotency keys" --repo $(pwd)
```

zornmesh parallel-spawns `claude --print`, `copilot -p`, `gemini --print`, and `opencode run` (all four by default; restrict with `--platforms claude,gemini`), pipes the same critique prompt into each, captures stdout with per-platform timeout and a 256 KiB memory cap, then prints an aggregated consensus to stdout. A JSONL audit record of every spawn lands in `$XDG_STATE_HOME/zornmesh/debates/<id>.jsonl` (or `~/.local/state/zornmesh/debates/<id>.jsonl`).

Each platform's outcome carries a stable `status` so partial results stay useful when one platform fails:

| Status | Meaning |
|---|---|
| `success` | exit 0 with stdout content |
| `empty_response` | exit 0 with no content |
| `non_zero_exit` | exit non-zero, stderr captured |
| `timeout` | exceeded `--timeout` (default 60s) — process group killed |
| `cli_missing` | binary not on PATH at debate start |
| `spawn_failed` | OS spawn error |

Replay any past debate from the audit log:

```bash
zornmesh debate replay deb-19ddf1f5c56-0001
```

### Calling it from inside a coding-agent host

Drivers in Claude Code, Copilot CLI, Gemini CLI, and OpenCode invoke the debate via their host's bash tool — no bespoke MCP integration needed. From any host, the prompt:

> Run `zornmesh debate run "<your plan>" --repo $(pwd) --output json` and synthesize the consensus into your final plan.

works end-to-end. Pipe the JSON output into the host's tool result; per-platform structured results let the model decide what to do with dissent.

### What got removed in v0.3

The v0.1/v0.2 substrate had a daemon, a pub/sub broker, per-platform worker daemons, a subject taxonomy, and an SDK-mediated cross-process orchestrator — all justified by hypothetical multi-host or cross-machine use cases that never materialized. v0.3 deletes:

- `zornmesh worker --platform <p>` (no persistent workers; debate spawns subprocesses on demand)
- `zornmesh service install` of worker units (nothing to supervise)
- The `agent.worker.<platform>` mesh-identity convention for debate participants
- `debate.<id>.plan` / `debate.<id>.critique.<agent>` / `debate.<id>.consensus` subject taxonomy (no broker)
- Hand-rolled envelope serde for plan/critique/consensus types
- The SDK-mediated `cli_runner` module

The daemon (`zornmesh daemon`) and MCP stdio bridge (`zornmesh stdio`) still exist for the broader local-mesh use cases the project may grow into; they are no longer involved in the debate flow.

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
