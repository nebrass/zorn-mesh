# Contributing to Zorn Mesh

## Development Setup

1. Clone the repository
2. Install dependencies: `npm install`
3. Build: `npm run build`
4. Run tests: `npm test`

## Code Style

- TypeScript strict mode enabled
- No `any` types — use `unknown` and type guards
- All public APIs must be typed

## Pull Requests

1. Fork and create a feature branch
2. Write tests for new functionality
3. Ensure `npm test` passes
4. Submit a pull request with a clear description

## Project Structure

- `src/core/` – Core message types, registry, router, store
- `src/transport/` – HTTP and STDIO transports
- `src/cli/` – CLI tool
- `tests/` – Test suite
- `examples/` – Usage examples
