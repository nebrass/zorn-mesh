import express, { Request, Response, NextFunction } from 'express';
import { Router as ExpressRouter } from 'express';
import { AgentRegistry } from '../core/registry';
import { Router } from '../core/router';
import { MessageStore } from '../core/store';
import { AgentInfo, MessageEnvelope, isValidMessage } from '../core/schema';

export function createHttpApp(
  registry: AgentRegistry,
  router: Router,
  store: MessageStore
): express.Application {
  const app = express();

  app.use(express.json());

  // Restrict to localhost only
  app.use((req: Request, res: Response, next: NextFunction) => {
    const ip = req.ip ?? req.socket.remoteAddress ?? '';
    const isLocal =
      ip === '127.0.0.1' ||
      ip === '::1' ||
      ip === '::ffff:127.0.0.1' ||
      ip.startsWith('127.');
    if (!isLocal) {
      res.status(403).json({ error: 'Access restricted to localhost' });
      return;
    }
    next();
  });

  const apiRouter = ExpressRouter();

  // Health check
  apiRouter.get('/health', (_req: Request, res: Response) => {
    res.json({ status: 'ok', timestamp: new Date().toISOString() });
  });

  // Agents
  apiRouter.get('/agents', (_req: Request, res: Response) => {
    res.json(registry.listAgents());
  });

  apiRouter.get('/agents/:id', (req: Request, res: Response) => {
    const agent = registry.getAgent(req.params['id']!);
    if (!agent) {
      res.status(404).json({ error: 'Agent not found' });
      return;
    }
    res.json(agent);
  });

  apiRouter.post('/agents', (req: Request, res: Response) => {
    const body = req.body as Partial<AgentInfo>;
    if (!body.id || !body.name) {
      res.status(400).json({ error: 'id and name are required' });
      return;
    }
    const now = new Date().toISOString();
    const agent: AgentInfo = {
      id: body.id,
      name: body.name,
      description: body.description,
      capabilities: body.capabilities,
      transport: body.transport ?? 'http',
      registeredAt: body.registeredAt ?? now,
      lastSeen: now,
    };
    registry.register(agent);
    res.status(201).json(agent);
  });

  apiRouter.delete('/agents/:id', (req: Request, res: Response) => {
    const removed = registry.unregister(req.params['id']!);
    if (!removed) {
      res.status(404).json({ error: 'Agent not found' });
      return;
    }
    res.status(204).send();
  });

  // Messages
  apiRouter.get('/messages', (req: Request, res: Response) => {
    const limit = parseInt((req.query['limit'] as string) ?? '100', 10);
    const agentId = req.query['agentId'] as string | undefined;
    const topic = req.query['topic'] as string | undefined;

    let messages: MessageEnvelope[];
    if (agentId) {
      messages = store.listByAgent(agentId, limit);
    } else if (topic) {
      messages = store.listByTopic(topic, limit);
    } else {
      messages = store.listAll(limit);
    }
    res.json(messages);
  });

  apiRouter.get('/messages/:id', (req: Request, res: Response) => {
    const msg = store.getById(req.params['id']!);
    if (!msg) {
      res.status(404).json({ error: 'Message not found' });
      return;
    }
    res.json(msg);
  });

  apiRouter.post('/messages', (req: Request, res: Response) => {
    const body = req.body as unknown;
    if (!isValidMessage(body)) {
      res.status(400).json({ error: 'Invalid message envelope' });
      return;
    }
    router.route(body);
    res.status(202).json({ id: body.id, status: 'routed' });
  });

  // Channels (pub/sub topics)
  apiRouter.get('/channels', (_req: Request, res: Response) => {
    const topics = router.listTopics();
    const channels = topics.map((topic) => ({
      topic,
      subscribers: router.getSubscribers(topic),
    }));
    res.json(channels);
  });

  apiRouter.post('/channels/:topic/subscribe', (req: Request, res: Response) => {
    const { agentId } = req.body as { agentId: string };
    if (!agentId) {
      res.status(400).json({ error: 'agentId required' });
      return;
    }
    router.subscribe(agentId, req.params['topic']!);
    res.status(201).json({ topic: req.params['topic'], agentId });
  });

  apiRouter.delete('/channels/:topic/subscribe/:agentId', (req: Request, res: Response) => {
    router.unsubscribe(req.params['agentId']!, req.params['topic']!);
    res.status(204).send();
  });

  app.use('/api', apiRouter);
  return app;
}

export function startHttpServer(port = 3737): void {
  const registry = new AgentRegistry();
  const store = new MessageStore();
  const router = new Router(registry, store);
  const app = createHttpApp(registry, router, store);

  app.listen(port, '127.0.0.1', () => {
    console.log(`Zorn Mesh HTTP server running on http://127.0.0.1:${port}`);
  });
}

// Allow running directly
if (require.main === module) {
  const port = parseInt(process.env['PORT'] ?? '3737', 10);
  startHttpServer(port);
}
