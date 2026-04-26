/**
 * Request/Reply Example
 * Alice sends a request to Bob and awaits a reply.
 */
import { AgentRegistry, Router, MessageStore, createMessage } from '../src/core';

async function main() {
  const registry = new AgentRegistry();
  const store = new MessageStore(':memory:');
  const router = new Router(registry, store);

  const now = new Date().toISOString();
  registry.register({ id: 'agent-alice', name: 'Alice', transport: 'internal', registeredAt: now, lastSeen: now });
  registry.register({ id: 'agent-bob', name: 'Bob', transport: 'internal', registeredAt: now, lastSeen: now });

  router.onAgentMessage('agent-bob', (msg) => {
    if (msg.type === 'request') {
      const payload = msg.payload as { query: string };
      const reply = createMessage('agent-bob', 'reply', { answer: `You asked: "${payload.query}"` }, {
        recipient: msg.sender,
        correlationId: msg.id,
      });
      router.route(reply);
    }
  });

  const request = createMessage('agent-alice', 'request', { query: 'What is the meaning of life?' }, {
    recipient: 'agent-bob',
  });

  const reply = await router.sendRequest(request, 3000);
  console.log('Alice received reply:', JSON.stringify(reply.payload));
}

main().catch(console.error);
