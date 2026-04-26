import * as readline from 'readline';
import { AgentRegistry } from '../core/registry';
import { Router } from '../core/router';
import { MessageStore } from '../core/store';
import { isValidMessage } from '../core/schema';

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: string | number;
  method: string;
  params?: unknown;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: string | number | null;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

function sendResponse(res: JsonRpcResponse): void {
  process.stdout.write(JSON.stringify(res) + '\n');
}

function ok(id: string | number, result: unknown): void {
  sendResponse({ jsonrpc: '2.0', id, result });
}

function err(id: string | number | null, code: number, message: string, data?: unknown): void {
  sendResponse({ jsonrpc: '2.0', id, error: { code, message, data } });
}

export function startStdioAdapter(
  registry: AgentRegistry,
  router: Router,
  store: MessageStore
): void {
  const rl = readline.createInterface({ input: process.stdin, terminal: false });

  rl.on('line', (line: string) => {
    let req: JsonRpcRequest;
    try {
      req = JSON.parse(line) as JsonRpcRequest;
    } catch {
      err(null, -32700, 'Parse error');
      return;
    }

    if (req.jsonrpc !== '2.0' || !req.method) {
      err(req.id ?? null, -32600, 'Invalid Request');
      return;
    }

    const params = req.params as Record<string, unknown> | undefined;

    switch (req.method) {
      case 'register': {
        const agentInfo = params as unknown as Parameters<typeof registry.register>[0];
        if (!agentInfo?.id || !agentInfo?.name) {
          err(req.id, -32602, 'Invalid params: id and name required');
          return;
        }
        const now = new Date().toISOString();
        registry.register({
          ...agentInfo,
          transport: agentInfo.transport ?? 'stdio',
          registeredAt: agentInfo.registeredAt ?? now,
          lastSeen: now,
        });
        ok(req.id, { registered: true, agentId: agentInfo.id });
        break;
      }

      case 'discover': {
        ok(req.id, registry.listAgents());
        break;
      }

      case 'send': {
        const msg = params as unknown;
        if (!isValidMessage(msg)) {
          err(req.id, -32602, 'Invalid message envelope');
          return;
        }
        router.route(msg);
        ok(req.id, { routed: true, id: msg.id });
        break;
      }

      case 'subscribe': {
        const { agentId, topic } = params as { agentId: string; topic: string };
        if (!agentId || !topic) {
          err(req.id, -32602, 'agentId and topic required');
          return;
        }
        router.subscribe(agentId, topic);
        ok(req.id, { subscribed: true, topic });
        break;
      }

      case 'unsubscribe': {
        const { agentId, topic } = params as { agentId: string; topic: string };
        if (!agentId || !topic) {
          err(req.id, -32602, 'agentId and topic required');
          return;
        }
        router.unsubscribe(agentId, topic);
        ok(req.id, { unsubscribed: true, topic });
        break;
      }

      case 'messages.list': {
        const limit = typeof params?.['limit'] === 'number' ? params['limit'] : 100;
        ok(req.id, store.listAll(limit));
        break;
      }

      case 'messages.get': {
        const id = params?.['id'] as string | undefined;
        if (!id) {
          err(req.id, -32602, 'id required');
          return;
        }
        const msg = store.getById(id);
        if (!msg) {
          err(req.id, -32001, 'Message not found');
          return;
        }
        ok(req.id, msg);
        break;
      }

      case 'channels.list': {
        ok(req.id, router.listTopics().map((t) => ({
          topic: t,
          subscribers: router.getSubscribers(t),
        })));
        break;
      }

      default:
        err(req.id, -32601, `Method not found: ${req.method}`);
    }
  });

  rl.on('close', () => {
    process.exit(0);
  });
}

if (require.main === module) {
  const registry = new AgentRegistry();
  const store = new MessageStore();
  const router = new Router(registry, store);
  startStdioAdapter(registry, router, store);
}
