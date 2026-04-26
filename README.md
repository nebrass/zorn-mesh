# Zorn Mesh

A lightweight local-first message bus for coding agents (GitHub Copilot, Claude, etc.) running on the same machine.

## Features

- **Agent Registry** – Register and discover local agents
- **Direct Messaging** – Send messages between specific agents
- **Request/Reply** – Synchronous-style request with async reply correlation
- **Pub/Sub** – Topic-based publish/subscribe channels
- **Message Persistence** – SQLite-backed message store with replay
- **HTTP API** – REST API restricted to localhost
- **STDIO/JSON-RPC** – JSON-RPC 2.0 over stdin/stdout for CLI integration
- **CLI** – `zorn` command-line tool

## Installation

```bash
npm install
npm run build
```

## Quick Start

Start the server:
```bash
npm start
# or
npx ts-node src/transport/http.ts
```

Use the CLI:
```bash
# Register an agent
node dist/cli/index.js agents register my-agent "My Agent" -c "code-gen,review"

# List agents
node dist/cli/index.js agents list

# Send a message
node dist/cli/index.js messages send agent-a agent-b '{"hello":"world"}'

# Check server status
node dist/cli/index.js server status
```

## API

### Agents

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/agents | List all agents |
| POST | /api/agents | Register an agent |
| GET | /api/agents/:id | Get agent info |
| DELETE | /api/agents/:id | Unregister agent |

### Messages

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/messages | List messages |
| POST | /api/messages | Route a message |
| GET | /api/messages/:id | Get message by ID |

### Channels (Pub/Sub)

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/channels | List active channels |
| POST | /api/channels/:topic/subscribe | Subscribe agent to topic |
| DELETE | /api/channels/:topic/subscribe/:agentId | Unsubscribe |

## Message Types

- `direct` – Point-to-point message
- `request` – Awaits a reply
- `reply` – Response to a request (correlationId links to request)
- `publish` – Broadcast to topic subscribers
- `subscribe` / `unsubscribe` – Manage topic subscriptions
- `register` – Register an agent via message
- `discover` – Discover available agents
- `error` – Error response

## Development

```bash
npm test          # Run tests
npm run build     # Compile TypeScript
npm run dev       # Start with ts-node
```

## Security

- HTTP server binds to `127.0.0.1` only
- All requests from non-localhost IPs are rejected with 403
- No external network access

## License

MIT
