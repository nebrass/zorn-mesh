/**
 * Pub/Sub Example
 * Multiple agents subscribe to a topic and one agent publishes to it.
 */
import { AgentRegistry, Router, MessageStore, createMessage } from '../src/core';

const registry = new AgentRegistry();
const store = new MessageStore(':memory:');
const router = new Router(registry, store);

const now = new Date().toISOString();
['agent-publisher', 'agent-sub1', 'agent-sub2'].forEach((id) =>
  registry.register({ id, name: id, transport: 'internal', registeredAt: now, lastSeen: now })
);

router.onAgentMessage('agent-sub1', (msg) => {
  if (msg.type === 'publish') console.log(`Sub1 received on topic "${msg.topic}": ${JSON.stringify(msg.payload)}`);
});
router.onAgentMessage('agent-sub2', (msg) => {
  if (msg.type === 'publish') console.log(`Sub2 received on topic "${msg.topic}": ${JSON.stringify(msg.payload)}`);
});

router.subscribe('agent-sub1', 'news');
router.subscribe('agent-sub2', 'news');

const pubMsg = createMessage('agent-publisher', 'publish', { headline: 'Zorn Mesh launched!' }, { topic: 'news' });
router.route(pubMsg);

console.log('Active topics:', router.listTopics());
console.log('Subscribers to news:', router.getSubscribers('news'));
