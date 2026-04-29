# Installing zornmesh

`zornmesh` is distributed as a single static-ish Rust binary. Pick whichever channel you prefer — they all install the same artifact.

## Channels

### Homebrew (macOS / Linux)

```bash
brew install nebrass/tap/zornmesh
```

The tap is auto-updated on every release tag by `.github/workflows/release.yml`. The formula pins SHA-256 of the macOS binaries and points at the GitHub Release assets.

### cargo-binstall (any Rust target)

```bash
cargo binstall zornmesh
```

`binstall` reads the `[package.metadata.binstall]` block in `crates/zornmesh-cli/Cargo.toml`, downloads the matching prebuilt artifact from the GitHub Release, and skips compilation entirely.

### crates.io (compile from source)

```bash
cargo install zornmesh
```

Slowest path (30–120s compile) but the most universal — works on any architecture Cargo supports. Requires a working Rust toolchain.

### Docker / GHCR

```bash
docker run --rm -i ghcr.io/nebrass/zornmesh:latest stdio --as-agent default
```

The image is multi-arch (linux/amd64, linux/arm64) and built on a distroless base for a minimal attack surface. Use this when you want isolation or have no Rust/Homebrew install on the host.

### Curl-installer / shell one-liner

```bash
curl -fsSL https://github.com/nebrass/zorn-mesh/releases/latest/download/zornmesh-x86_64-unknown-linux-gnu.tar.gz \
  | tar -xz -C /tmp && sudo mv /tmp/zornmesh-x86_64-unknown-linux-gnu/zornmesh /usr/local/bin/
```

Manual but transparent. The release also publishes `SHA256SUMS` next to the archives for verification.

## Wiring it into an MCP host

`zornmesh` exposes a Model Context Protocol server over stdio: `zornmesh stdio --as-agent <id>`. The agent id identifies the host within the local mesh.

| Host | File | Top-level key |
|---|---|---|
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Code | `~/.claude.json` (user) or `.mcp.json` (project) | `mcpServers` |
| Cursor | `~/.cursor/mcp.json` or `.cursor/mcp.json` | `mcpServers` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` |
| VS Code (Copilot) | `.vscode/mcp.json` | `servers` (note the difference) |

Standard config block:

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

For VS Code, replace `mcpServers` with `servers`.

For Claude Code, the CLI shortcut writes the JSON for you:

```bash
claude mcp add zornmesh -- zornmesh stdio --as-agent default
```

## Bootstrap behavior

`zornmesh stdio` connects to the per-user daemon on its Unix-domain socket. If no daemon is running:

- **Default:** the bridge autospawns `zornmesh daemon` detached and waits up to 5s for the readiness line. Override the timeout with `ZORN_AUTOSPAWN_TIMEOUT_MS=<ms>`.
- **Opt-out:** set `ZORN_NO_AUTOSPAWN=1` to fail fast with a stable error pointing at `zornmesh service install`.

## Login-persistent daemon

To keep the daemon alive across logins (so the first stdio session of the day doesn't pay the spawn cost):

```bash
zornmesh service install
```

This writes a launchd plist (macOS) or systemd user unit (Linux) and prints the exact activation command — `launchctl bootstrap gui/$UID …` or `systemctl --user enable --now zornmesh.service`. No automatic activation; you audit and run the activation step yourself.

To remove:

```bash
zornmesh service uninstall
```

To inspect:

```bash
zornmesh service status
```

## Verifying the install

```bash
zornmesh --version
zornmesh stdio --help
zornmesh service status
```
