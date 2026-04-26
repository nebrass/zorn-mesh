import Database from 'better-sqlite3';
import path from 'path';
import os from 'os';
import fs from 'fs';
import { MessageEnvelope } from './schema';

export interface StoreFilter {
  agentId?: string;
  topic?: string;
  type?: string;
  since?: string;
  limit?: number;
}

export class MessageStore {
  private db: Database.Database;

  constructor(dbPath?: string) {
    const resolvedPath = dbPath ?? path.join(os.homedir(), '.zorn-mesh', 'messages.db');
    if (resolvedPath !== ':memory:') {
      const dir = path.dirname(resolvedPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
    }
    this.db = new Database(resolvedPath);
    this.init();
  }

  private init(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        sender TEXT NOT NULL,
        recipient TEXT,
        topic TEXT,
        type TEXT NOT NULL,
        correlation_id TEXT,
        timestamp TEXT NOT NULL,
        payload TEXT NOT NULL,
        ttl INTEGER
      );
      CREATE INDEX IF NOT EXISTS idx_sender ON messages(sender);
      CREATE INDEX IF NOT EXISTS idx_recipient ON messages(recipient);
      CREATE INDEX IF NOT EXISTS idx_topic ON messages(topic);
      CREATE INDEX IF NOT EXISTS idx_timestamp ON messages(timestamp);
    `);
  }

  save(msg: MessageEnvelope): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO messages
        (id, sender, recipient, topic, type, correlation_id, timestamp, payload, ttl)
      VALUES
        (@id, @sender, @recipient, @topic, @type, @correlationId, @timestamp, @payload, @ttl)
    `);
    stmt.run({
      id: msg.id,
      sender: msg.sender,
      recipient: msg.recipient ?? null,
      topic: msg.topic ?? null,
      type: msg.type,
      correlationId: msg.correlationId ?? null,
      timestamp: msg.timestamp,
      payload: JSON.stringify(msg.payload),
      ttl: msg.ttl ?? null,
    });
  }

  getById(id: string): MessageEnvelope | undefined {
    const row = this.db.prepare('SELECT * FROM messages WHERE id = ?').get(id) as Record<string, unknown> | undefined;
    return row ? this.rowToEnvelope(row) : undefined;
  }

  listByAgent(agentId: string, limit = 100): MessageEnvelope[] {
    const rows = this.db
      .prepare('SELECT * FROM messages WHERE sender = ? OR recipient = ? ORDER BY timestamp DESC LIMIT ?')
      .all(agentId, agentId, limit) as Record<string, unknown>[];
    return rows.map((r) => this.rowToEnvelope(r));
  }

  listByTopic(topic: string, limit = 100): MessageEnvelope[] {
    const rows = this.db
      .prepare('SELECT * FROM messages WHERE topic = ? ORDER BY timestamp DESC LIMIT ?')
      .all(topic, limit) as Record<string, unknown>[];
    return rows.map((r) => this.rowToEnvelope(r));
  }

  listAll(limit = 100): MessageEnvelope[] {
    const rows = this.db
      .prepare('SELECT * FROM messages ORDER BY timestamp DESC LIMIT ?')
      .all(limit) as Record<string, unknown>[];
    return rows.map((r) => this.rowToEnvelope(r));
  }

  replay(filter: StoreFilter): MessageEnvelope[] {
    let query = 'SELECT * FROM messages WHERE 1=1';
    const params: unknown[] = [];
    if (filter.agentId) {
      query += ' AND (sender = ? OR recipient = ?)';
      params.push(filter.agentId, filter.agentId);
    }
    if (filter.topic) {
      query += ' AND topic = ?';
      params.push(filter.topic);
    }
    if (filter.type) {
      query += ' AND type = ?';
      params.push(filter.type);
    }
    if (filter.since) {
      query += ' AND timestamp >= ?';
      params.push(filter.since);
    }
    query += ' ORDER BY timestamp ASC';
    if (filter.limit) {
      query += ' LIMIT ?';
      params.push(filter.limit);
    }
    const rows = this.db.prepare(query).all(...params) as Record<string, unknown>[];
    return rows.map((r) => this.rowToEnvelope(r));
  }

  close(): void {
    this.db.close();
  }

  private rowToEnvelope(row: Record<string, unknown>): MessageEnvelope {
    return {
      id: row['id'] as string,
      sender: row['sender'] as string,
      recipient: (row['recipient'] as string | null) ?? undefined,
      topic: (row['topic'] as string | null) ?? undefined,
      type: row['type'] as MessageEnvelope['type'],
      correlationId: (row['correlation_id'] as string | null) ?? undefined,
      timestamp: row['timestamp'] as string,
      payload: JSON.parse(row['payload'] as string),
      ttl: (row['ttl'] as number | null) ?? undefined,
    };
  }
}
