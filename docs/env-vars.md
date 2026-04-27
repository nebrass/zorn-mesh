# Environment variables

Zorn Mesh recognizes these local lifecycle environment variables.

| Variable | Purpose |
| --- | --- |
| `ZORN_SOCKET_PATH` | Overrides the resolved local Unix-domain socket path used by the daemon, CLI, and Rust SDK connect validation. |
| `ZORN_NO_AUTOSPAWN=1` | Disables SDK/CLI auto-spawn behavior. When no daemon is reachable, clients return `E_DAEMON_UNREACHABLE` with a remediation hint instead of creating a socket or launching a daemon. |
| `ZORN_SHUTDOWN_BUDGET_MS` | Configures the daemon shutdown drain budget in milliseconds. Values are capped at 60 seconds; the default is 10 seconds. |

The default Linux socket path is `${XDG_RUNTIME_DIR}/zorn-mesh/zorn.sock`, falling back to `/run/user/$UID/zorn-mesh/zorn.sock`.
