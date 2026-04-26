import { Router } from '../src/core/router';
import { AgentRegistry } from '../src/core/registry';
import { MessageStore } from '../src/core/store';
import { createMessage, MessageEnvelope } from '../src/core/schema';

function makeSetup() {
  const registry = new AgentRegistry();
  const store = new MessageStore(':memory:');
  const router = new Router(registry, store);
  const now = new Date().toISOString();
  registry.register({ id: 'alice', name: 'Alice', transport: 'internal', registeredAt: now, lastSeen: now });
  registry.register({ id: 'bob', name: 'Bob', transport: 'internal', registeredAt: now, lastSeen: now });
  return { registry, store, router };
}

describe('Router - direct messaging', () => {
  let store: MessageStore;
  afterEach(() => store.close());

  test('routes direct message to recipient', () => {
    const { router, store: s } = makeSetup();
    store = s;
    const received: MessageEnvelope[] = [];
    router.onAgentMessage('bob', (msg) => received.push(msg));
    const msg = createMessage('alice', 'direct', { text: 'hi' }, { recipient: 'bob' });
    router.route(msg);
    expect(received).toHaveLength(1);
    expect(received[0]?.payload).toEqual({ text: 'hi' });
  });

  test('persists message in store', () => {
    const { router, store: s } = makeSetup();
    store = s;
    const msg = createMessage('alice', 'direct', {}, { recipient: 'bob' });
    router.route(msg);
    expect(store.getById(msg.id)).toBeDefined();
  });
});

describe('Router - pub/sub', () => {
  let store: MessageStore;
  afterEach(() => store.close());

  test('delivers to subscribers only', () => {
    const { router, store: s } = makeSetup();
    store = s;
    const received: MessageEnvelope[] = [];
    router.onAgentMessage('bob', (msg) => received.push(msg));
    router.subscribe('bob', 'events');

    const msg = createMessage('alice', 'publish', { event: 'test' }, { topic: 'events' });
    router.route(msg);
    expect(received).toHaveLength(1);
  });

  test('does not deliver to publisher', () => {
    const { router, store: s } = makeSetup();
    store = s;
    router.subscribe('alice', 'events');
    router.subscribe('bob', 'events');
    const received: MessageEnvelope[] = [];
    router.onAgentMessage('alice', (msg) => received.push(msg));
    router.route(createMessage('alice', 'publish', {}, { topic: 'events' }));
    expect(received).toHaveLength(0);
  });

  test('unsubscribe stops delivery', () => {
    const { router, store: s } = makeSetup();
    store = s;
    router.subscribe('bob', 'events');
    router.unsubscribe('bob', 'events');
    const received: MessageEnvelope[] = [];
    router.onAgentMessage('bob', (msg) => received.push(msg));
    router.route(createMessage('alice', 'publish', {}, { topic: 'events' }));
    expect(received).toHaveLength(0);
  });
});

describe('Router - request/reply', () => {
  let store: MessageStore;
  afterEach(() => store.close());

  test('resolves reply via correlationId', async () => {
    const { router, store: s } = makeSetup();
    store = s;
    router.onAgentMessage('bob', (msg) => {
      if (msg.type === 'request') {
        const reply = createMessage('bob', 'reply', { answer: 42 }, {
          recipient: msg.sender,
          correlationId: msg.id,
        });
        setTimeout(() => router.route(reply), 10);
      }
    });

    const req = createMessage('alice', 'request', { q: 'life?' }, { recipient: 'bob' });
    const reply = await router.sendRequest(req, 2000);
    expect(reply.payload).toEqual({ answer: 42 });
  });

  test('times out if no reply', async () => {
    const { router, store: s } = makeSetup();
    store = s;
    const req = createMessage('alice', 'request', {}, { recipient: 'bob' });
    await expect(router.sendRequest(req, 100)).rejects.toThrow('timed out');
  });
});

describe('Router - topics', () => {
  let store: MessageStore;
  afterEach(() => store.close());

  test('listTopics returns active topics', () => {
    const { router, store: s } = makeSetup();
    store = s;
    router.subscribe('bob', 'a');
    router.subscribe('alice', 'b');
    expect(router.listTopics()).toContain('a');
    expect(router.listTopics()).toContain('b');
  });

  test('getSubscribers returns correct agents', () => {
    const { router, store: s } = makeSetup();
    store = s;
    router.subscribe('bob', 'news');
    router.subscribe('alice', 'news');
    expect(router.getSubscribers('news')).toContain('bob');
    expect(router.getSubscribers('news')).toContain('alice');
  });
});
