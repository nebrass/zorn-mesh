import { EventEmitter } from 'events';
import { MessageEnvelope, createMessage } from './schema';
import { AgentRegistry } from './registry';
import { MessageStore } from './store';

type MessageHandler = (msg: MessageEnvelope) => void;

export class Router extends EventEmitter {
  private subscriptions: Map<string, Set<string>> = new Map();
  private pendingReplies: Map<string, MessageHandler> = new Map();

  constructor(
    private registry: AgentRegistry,
    private store: MessageStore
  ) {
    super();
  }

  route(msg: MessageEnvelope): void {
    this.store.save(msg);
    this.registry.updateLastSeen(msg.sender);

    switch (msg.type) {
      case 'direct':
      case 'request':
        this.routeDirect(msg);
        break;
      case 'reply':
        this.routeReply(msg);
        break;
      case 'publish':
        this.routePublish(msg);
        break;
      case 'subscribe':
        this.handleSubscribe(msg);
        break;
      case 'unsubscribe':
        this.handleUnsubscribe(msg);
        break;
      case 'register':
        this.handleRegister(msg);
        break;
      case 'discover':
        this.handleDiscover(msg);
        break;
      default:
        this.emit('error', new Error(`Unknown message type: ${msg.type}`));
    }
    this.emit('message', msg);
  }

  private routeDirect(msg: MessageEnvelope): void {
    if (!msg.recipient) {
      this.emitError(msg, 'Direct/request message missing recipient');
      return;
    }
    this.emit(`agent:${msg.recipient}`, msg);
    this.emit('deliver', msg);
  }

  private routeReply(msg: MessageEnvelope): void {
    if (msg.correlationId) {
      const handler = this.pendingReplies.get(msg.correlationId);
      if (handler) {
        handler(msg);
        this.pendingReplies.delete(msg.correlationId);
      }
    }
    if (msg.recipient) {
      this.emit(`agent:${msg.recipient}`, msg);
    }
    this.emit('deliver', msg);
  }

  private routePublish(msg: MessageEnvelope): void {
    if (!msg.topic) {
      this.emitError(msg, 'Publish message missing topic');
      return;
    }
    const subs = this.subscriptions.get(msg.topic) ?? new Set();
    for (const agentId of subs) {
      if (agentId !== msg.sender) {
        this.emit(`agent:${agentId}`, msg);
      }
    }
    this.emit(`topic:${msg.topic}`, msg);
    this.emit('deliver', msg);
  }

  private handleSubscribe(msg: MessageEnvelope): void {
    const topic = msg.topic ?? (msg.payload as Record<string, string>)?.['topic'];
    if (!topic) return;
    if (!this.subscriptions.has(topic)) {
      this.subscriptions.set(topic, new Set());
    }
    this.subscriptions.get(topic)!.add(msg.sender);
    this.emit('subscribe', { agentId: msg.sender, topic });
  }

  private handleUnsubscribe(msg: MessageEnvelope): void {
    const topic = msg.topic ?? (msg.payload as Record<string, string>)?.['topic'];
    if (!topic) return;
    this.subscriptions.get(topic)?.delete(msg.sender);
    this.emit('unsubscribe', { agentId: msg.sender, topic });
  }

  private handleRegister(msg: MessageEnvelope): void {
    const info = msg.payload as Parameters<AgentRegistry['register']>[0];
    this.registry.register(info);
  }

  private handleDiscover(msg: MessageEnvelope): void {
    const agents = this.registry.listAgents();
    const reply = createMessage('system', 'reply', agents, {
      recipient: msg.sender,
      correlationId: msg.id,
    });
    this.route(reply);
  }

  sendRequest(
    msg: MessageEnvelope,
    timeoutMs = 5000
  ): Promise<MessageEnvelope> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingReplies.delete(msg.id);
        reject(new Error(`Request ${msg.id} timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      this.pendingReplies.set(msg.id, (reply) => {
        clearTimeout(timer);
        resolve(reply);
      });

      this.route(msg);
    });
  }

  subscribe(agentId: string, topic: string): void {
    if (!this.subscriptions.has(topic)) {
      this.subscriptions.set(topic, new Set());
    }
    this.subscriptions.get(topic)!.add(agentId);
    this.emit('subscribe', { agentId, topic });
  }

  unsubscribe(agentId: string, topic: string): void {
    this.subscriptions.get(topic)?.delete(agentId);
    this.emit('unsubscribe', { agentId, topic });
  }

  listTopics(): string[] {
    return Array.from(this.subscriptions.keys()).filter(
      (t) => (this.subscriptions.get(t)?.size ?? 0) > 0
    );
  }

  getSubscribers(topic: string): string[] {
    return Array.from(this.subscriptions.get(topic) ?? []);
  }

  onAgentMessage(agentId: string, handler: MessageHandler): void {
    this.on(`agent:${agentId}`, handler);
  }

  offAgentMessage(agentId: string, handler: MessageHandler): void {
    this.off(`agent:${agentId}`, handler);
  }

  private emitError(msg: MessageEnvelope, reason: string): void {
    const errMsg = createMessage('system', 'error', { reason, originalId: msg.id }, {
      recipient: msg.sender,
      correlationId: msg.id,
    });
    this.store.save(errMsg);
    this.emit(`agent:${msg.sender}`, errMsg);
  }
}
