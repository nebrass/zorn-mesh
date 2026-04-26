#!/usr/bin/env node
import { Command } from 'commander';

const BASE_URL = process.env['ZORN_SERVER'] ?? 'http://127.0.0.1:3737/api';

async function apiFetch(path: string, options?: RequestInit): Promise<unknown> {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  if (res.status === 204) return null;
  return res.json();
}

const program = new Command();

program
  .name('zorn')
  .description('Zorn Mesh - Local agent message bus CLI')
  .version('0.1.0');

// Agents
const agentsCmd = program.command('agents').description('Manage agents');

agentsCmd
  .command('list')
  .description('List all registered agents')
  .action(async () => {
    try {
      const agents = await apiFetch('/agents');
      console.log(JSON.stringify(agents, null, 2));
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

agentsCmd
  .command('register <id> <name>')
  .description('Register an agent')
  .option('-d, --description <desc>', 'Agent description')
  .option('-c, --capabilities <caps>', 'Comma-separated capabilities')
  .action(async (id: string, name: string, opts: { description?: string; capabilities?: string }) => {
    try {
      const agent = await apiFetch('/agents', {
        method: 'POST',
        body: JSON.stringify({
          id,
          name,
          description: opts.description,
          capabilities: opts.capabilities?.split(',').map((c) => c.trim()),
          transport: 'http',
        }),
      });
      console.log(JSON.stringify(agent, null, 2));
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

agentsCmd
  .command('unregister <id>')
  .description('Unregister an agent')
  .action(async (id: string) => {
    try {
      await apiFetch(`/agents/${id}`, { method: 'DELETE' });
      console.log(`Agent ${id} unregistered.`);
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

// Messages
const messagesCmd = program.command('messages').description('Inspect messages');

messagesCmd
  .command('list')
  .description('List messages')
  .option('-l, --limit <n>', 'Max number of messages', '20')
  .option('-a, --agent <id>', 'Filter by agent ID')
  .option('-t, --topic <topic>', 'Filter by topic')
  .action(async (opts: { limit: string; agent?: string; topic?: string }) => {
    try {
      const params = new URLSearchParams({ limit: opts.limit });
      if (opts.agent) params.set('agentId', opts.agent);
      if (opts.topic) params.set('topic', opts.topic);
      const messages = await apiFetch(`/messages?${params}`);
      console.log(JSON.stringify(messages, null, 2));
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

messagesCmd
  .command('send <from> <to> <payload>')
  .description('Send a direct message')
  .action(async (from: string, to: string, payloadStr: string) => {
    try {
      let payload: unknown;
      try { payload = JSON.parse(payloadStr); } catch { payload = payloadStr; }
      const { v4: uuidv4 } = await import('uuid');
      const msg = {
        id: uuidv4(),
        sender: from,
        recipient: to,
        type: 'direct',
        timestamp: new Date().toISOString(),
        payload,
      };
      const result = await apiFetch('/messages', {
        method: 'POST',
        body: JSON.stringify(msg),
      });
      console.log(JSON.stringify(result, null, 2));
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

// Channels
const channelsCmd = program.command('channels').description('Manage pub/sub channels');

channelsCmd
  .command('list')
  .description('List active channels')
  .action(async () => {
    try {
      const channels = await apiFetch('/channels');
      console.log(JSON.stringify(channels, null, 2));
    } catch (e) {
      console.error('Error:', (e as Error).message);
      process.exit(1);
    }
  });

// Server
const serverCmd = program.command('server').description('Manage the Zorn Mesh server');

serverCmd
  .command('start')
  .description('Start the HTTP server')
  .option('-p, --port <n>', 'Port number', '3737')
  .action(async (opts: { port: string }) => {
    const { startHttpServer } = await import('../transport/http');
    startHttpServer(parseInt(opts.port, 10));
  });

serverCmd
  .command('status')
  .description('Check server status')
  .action(async () => {
    try {
      const health = await apiFetch('/health');
      console.log('Server is running:', JSON.stringify(health, null, 2));
    } catch {
      console.log('Server is not running or unreachable.');
      process.exit(1);
    }
  });

program.parse(process.argv);
