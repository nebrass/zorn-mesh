.PHONY: build release docker docker-run

build:
	@command -v cargo >/dev/null || { echo "missing required tool: cargo" >&2; exit 127; }
	cargo build --workspace --all-targets

release:
	@command -v cargo >/dev/null || { echo "missing required tool: cargo" >&2; exit 127; }
	cargo build --release -p zornmesh
	@echo "Release binary at: target/release/zornmesh"

docker:
	@command -v docker >/dev/null || { echo "missing required tool: docker" >&2; exit 127; }
	docker build -t zornmesh:dev .

docker-run: docker
	docker run --rm -i zornmesh:dev stdio --as-agent default
