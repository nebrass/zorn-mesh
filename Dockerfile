# Multi-stage build for the zornmesh CLI / MCP stdio bridge.
# Final stage is distroless for a minimal attack surface (~25MB).

FROM rust:1-slim-bookworm AS builder
WORKDIR /src

# Cache dependencies separately from sources.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY conformance ./conformance
COPY test-infra ./test-infra
COPY fixtures ./fixtures
COPY apps ./apps
COPY sdks ./sdks

ENV CARGO_INCREMENTAL=0
RUN cargo build --release -p zornmesh

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /src/target/release/zornmesh /usr/local/bin/zornmesh

# When invoked by an MCP host, stdin/stdout carry JSON-RPC framing.
# The default arg list assumes the host wants the stdio bridge.
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/zornmesh"]
CMD ["stdio", "--as-agent", "default"]
