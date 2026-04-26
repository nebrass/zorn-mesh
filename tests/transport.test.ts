import { createHttpApp } from '../src/transport/http';
import { AgentRegistry } from '../src/core/registry';
import { Router } from '../src/core/router';
import { MessageStore } from '../src/core/store';
import { createMessage } from '../src/core/schema';
import http from 'http';
import { AddressInfo } from 'net';

function postJSON(url: string, body: unknown): Promise<{ status: number; body: unknown }> {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify(body);
    const urlObj = new URL(url);
    const options = {
      hostname: urlObj.hostname,
      port: urlObj.port,
      path: urlObj.pathname,
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(data) },
    };
    const req = http.request(options, (res) => {
      let body = '';
      res.on('data', (d) => (body += d));
      res.on('end', () => {
        try { resolve({ status: res.statusCode!, body: JSON.parse(body) }); }
        catch { resolve({ status: res.statusCode!, body }); }
      });
    });
    req.on('error', reject);
    req.write(data);
    req.end();
  });
}

function getJSON(url: string): Promise<{ status: number; body: unknown }> {
  return new Promise((resolve, reject) => {
    const urlObj = new URL(url);
    const options = {
      hostname: urlObj.hostname,
      port: urlObj.port,
      path: urlObj.pathname + urlObj.search,
      method: 'GET',
      headers: { 'Accept': 'application/json' },
    };
    http.get(options, (res) => {
      let body = '';
      res.on('data', (d) => (body += d));
      res.on('end', () => {
        try { resolve({ status: res.statusCode!, body: JSON.parse(body) }); }
        catch { resolve({ status: res.statusCode!, body }); }
      });
    }).on('error', reject);
  });
}

describe('HTTP Transport', () => {
  let server: http.Server;
  let baseUrl: string;
  let store: MessageStore;

  beforeEach((done) => {
    const registry = new AgentRegistry();
    store = new MessageStore(':memory:');
    const router = new Router(registry, store);
    const app = createHttpApp(registry, router, store);
    server = app.listen(0, '127.0.0.1', () => {
      const port = (server.address() as AddressInfo).port;
      baseUrl = `http://127.0.0.1:${port}/api`;
      done();
    });
  });

  afterEach((done) => {
    store.close();
    server.close(done);
  });

  test('GET /health returns ok', async () => {
    const res = await getJSON(`${baseUrl}/health`);
    expect(res.status).toBe(200);
    expect((res.body as { status: string }).status).toBe('ok');
  });

  test('POST /agents registers agent', async () => {
    const res = await postJSON(`${baseUrl}/agents`, { id: 'test-agent', name: 'Test Agent', transport: 'http' });
    expect(res.status).toBe(201);
    expect((res.body as { id: string }).id).toBe('test-agent');
  });

  test('GET /agents lists agents', async () => {
    await postJSON(`${baseUrl}/agents`, { id: 'a1', name: 'A1', transport: 'http' });
    const res = await getJSON(`${baseUrl}/agents`);
    expect(res.status).toBe(200);
    expect((res.body as unknown[]).length).toBeGreaterThanOrEqual(1);
  });

  test('GET /agents/:id returns agent', async () => {
    await postJSON(`${baseUrl}/agents`, { id: 'agent-foo', name: 'Foo', transport: 'http' });
    const res = await getJSON(`${baseUrl}/agents/agent-foo`);
    expect(res.status).toBe(200);
    expect((res.body as { name: string }).name).toBe('Foo');
  });

  test('GET /agents/:id returns 404 for unknown', async () => {
    const res = await getJSON(`${baseUrl}/agents/nobody`);
    expect(res.status).toBe(404);
  });

  test('POST /messages routes message', async () => {
    await postJSON(`${baseUrl}/agents`, { id: 'sender', name: 'Sender', transport: 'http' });
    const msg = createMessage('sender', 'direct', { text: 'hello' }, { recipient: 'receiver' });
    const res = await postJSON(`${baseUrl}/messages`, msg);
    expect(res.status).toBe(202);
    expect((res.body as { status: string }).status).toBe('routed');
  });

  test('GET /messages lists messages', async () => {
    await postJSON(`${baseUrl}/agents`, { id: 'sender2', name: 'Sender2', transport: 'http' });
    const msg = createMessage('sender2', 'direct', {});
    await postJSON(`${baseUrl}/messages`, msg);
    const res = await getJSON(`${baseUrl}/messages`);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  test('GET /channels lists channels', async () => {
    const res = await getJSON(`${baseUrl}/channels`);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  test('POST /channels/:topic/subscribe subscribes agent', async () => {
    const res = await postJSON(`${baseUrl}/channels/test-topic/subscribe`, { agentId: 'agent-x' });
    expect(res.status).toBe(201);
  });
});
