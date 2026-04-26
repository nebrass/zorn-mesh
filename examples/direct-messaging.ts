/**
 * Direct Messaging Example
 * Shows two agents exchanging direct messages via the internal router.
 */
import { AgentRegistry, Router, MessageStore, createMessage } from '../src/core';

const registry = new AgentRegistry();
const store = new MessageStore(':memory:');
const router = new Router(registry, store);

const now = new Date().toISOString();
registry.register({ id: 'agent-alice', name: 'Alice', transport: 'internal', registeredAt: now, lastSeen: now });
registry.register({ id: 'agent-bob', name: 'Bob', transport: 'internal', registeredAt: now, lastSeen: now });

router.onAgentMessage('agent-bob', (msg) => {
  console.log(`Bob received: ${JSON.stringify(msg.payload)}`);
});

const msg = createMessage('agent-alice', 'direct', { text: 'Hello, Bob!' }, { recipient: 'agent-bob' });
router.route(msg);
console.log(`Alice sent message id=${msg.id}`);
