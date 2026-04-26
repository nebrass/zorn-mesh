import { createMessage, isValidMessage } from '../src/core/schema';
import { AgentRegistry } from '../src/core/registry';
import { MessageStore } from '../src/core/store';

describe('Schema', () => {
  test('createMessage generates valid envelope', () => {
    const msg = createMessage('agent-a', 'direct', { hello: 'world' }, { recipient: 'agent-b' });
    expect(msg.id).toBeTruthy();
    expect(msg.sender).toBe('agent-a');
    expect(msg.type).toBe('direct');
    expect(msg.recipient).toBe('agent-b');
    expect(msg.timestamp).toBeTruthy();
    expect(msg.payload).toEqual({ hello: 'world' });
  });

  test('isValidMessage returns true for valid messages', () => {
    const msg = createMessage('a', 'direct', {});
    expect(isValidMessage(msg)).toBe(true);
  });

  test('isValidMessage returns false for invalid messages', () => {
    expect(isValidMessage(null)).toBe(false);
    expect(isValidMessage({ id: 1 })).toBe(false);
    expect(isValidMessage('string')).toBe(false);
  });
});

describe('AgentRegistry', () => {
  let registry: AgentRegistry;
  const now = new Date().toISOString();
  const agentA = { id: 'a', name: 'Agent A', transport: 'internal' as const, registeredAt: now, lastSeen: now };
  const agentB = { id: 'b', name: 'Agent B', capabilities: ['code-gen'], transport: 'http' as const, registeredAt: now, lastSeen: now };

  beforeEach(() => { registry = new AgentRegistry(); });

  test('register and getAgent', () => {
    registry.register(agentA);
    expect(registry.getAgent('a')).toEqual(agentA);
  });

  test('unregister removes agent', () => {
    registry.register(agentA);
    expect(registry.unregister('a')).toBe(true);
    expect(registry.getAgent('a')).toBeUndefined();
  });

  test('unregister returns false for unknown agent', () => {
    expect(registry.unregister('nonexistent')).toBe(false);
  });

  test('listAgents returns all agents', () => {
    registry.register(agentA);
    registry.register(agentB);
    expect(registry.listAgents()).toHaveLength(2);
  });

  test('findByCapability filters correctly', () => {
    registry.register(agentA);
    registry.register(agentB);
    expect(registry.findByCapability('code-gen')).toHaveLength(1);
    expect(registry.findByCapability('code-gen')[0]?.id).toBe('b');
    expect(registry.findByCapability('unknown')).toHaveLength(0);
  });

  test('emits register event', () => {
    const handler = jest.fn();
    registry.on('register', handler);
    registry.register(agentA);
    expect(handler).toHaveBeenCalledWith(agentA);
  });

  test('emits unregister event', () => {
    const handler = jest.fn();
    registry.on('unregister', handler);
    registry.register(agentA);
    registry.unregister('a');
    expect(handler).toHaveBeenCalledWith(agentA);
  });
});

describe('MessageStore', () => {
  let store: MessageStore;

  beforeEach(() => { store = new MessageStore(':memory:'); });
  afterEach(() => { store.close(); });

  test('save and getById', () => {
    const msg = createMessage('a', 'direct', { x: 1 }, { recipient: 'b' });
    store.save(msg);
    const retrieved = store.getById(msg.id);
    expect(retrieved).toBeDefined();
    expect(retrieved?.id).toBe(msg.id);
    expect(retrieved?.payload).toEqual({ x: 1 });
  });

  test('listByAgent', () => {
    const m1 = createMessage('agent-x', 'direct', {}, { recipient: 'agent-y' });
    const m2 = createMessage('agent-y', 'direct', {}, { recipient: 'agent-x' });
    const m3 = createMessage('agent-z', 'direct', {});
    store.save(m1); store.save(m2); store.save(m3);
    const results = store.listByAgent('agent-x');
    expect(results).toHaveLength(2);
  });

  test('listByTopic', () => {
    const m1 = createMessage('a', 'publish', {}, { topic: 'news' });
    const m2 = createMessage('b', 'publish', {}, { topic: 'news' });
    const m3 = createMessage('c', 'publish', {}, { topic: 'sports' });
    store.save(m1); store.save(m2); store.save(m3);
    expect(store.listByTopic('news')).toHaveLength(2);
    expect(store.listByTopic('sports')).toHaveLength(1);
  });

  test('listAll with limit', () => {
    for (let i = 0; i < 5; i++) store.save(createMessage('a', 'direct', { i }));
    expect(store.listAll(3)).toHaveLength(3);
  });

  test('replay with filter', () => {
    const m1 = createMessage('a', 'direct', {}, { recipient: 'b' });
    const m2 = createMessage('a', 'publish', {}, { topic: 'news' });
    store.save(m1); store.save(m2);
    const results = store.replay({ agentId: 'a', type: 'publish' });
    expect(results).toHaveLength(1);
    expect(results[0]?.type).toBe('publish');
  });
});
