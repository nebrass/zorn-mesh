import { v4 as uuidv4 } from 'uuid';

export type MessageType =
  | 'direct'
  | 'request'
  | 'reply'
  | 'publish'
  | 'subscribe'
  | 'unsubscribe'
  | 'register'
  | 'discover'
  | 'error';

export interface MessageEnvelope {
  id: string;
  sender: string;
  recipient?: string;
  topic?: string;
  type: MessageType;
  correlationId?: string;
  timestamp: string;
  payload: unknown;
  ttl?: number;
}

export interface AgentInfo {
  id: string;
  name: string;
  description?: string;
  capabilities?: string[];
  registeredAt: string;
  lastSeen: string;
  transport: 'stdio' | 'http' | 'internal';
}

export function createMessage(
  sender: string,
  type: MessageType,
  payload: unknown,
  options: Partial<Omit<MessageEnvelope, 'id' | 'sender' | 'type' | 'payload' | 'timestamp'>> = {}
): MessageEnvelope {
  return {
    id: uuidv4(),
    sender,
    type,
    payload,
    timestamp: new Date().toISOString(),
    ...options,
  };
}

export function isValidMessage(msg: unknown): msg is MessageEnvelope {
  if (typeof msg !== 'object' || msg === null) return false;
  const m = msg as Record<string, unknown>;
  return (
    typeof m['id'] === 'string' &&
    typeof m['sender'] === 'string' &&
    typeof m['type'] === 'string' &&
    typeof m['timestamp'] === 'string' &&
    'payload' in m
  );
}
