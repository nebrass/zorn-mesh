set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

_tool tool:
    @command -v {{tool}} >/dev/null || { echo "missing required tool: {{tool}}" >&2; exit 127; }

check: (_tool "cargo")
    cargo xtask check

test: (_tool "cargo") (_tool "bun")
    cargo xtask test

lint: (_tool "cargo")
    cargo xtask lint

docs: (_tool "cargo")
    cargo xtask docs

conformance: (_tool "cargo")
    cargo xtask conformance
