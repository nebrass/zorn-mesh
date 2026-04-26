import { EventEmitter } from 'events';
import { AgentInfo } from './schema';

export class AgentRegistry extends EventEmitter {
  private agents: Map<string, AgentInfo> = new Map();

  register(agent: AgentInfo): void {
    this.agents.set(agent.id, agent);
    this.emit('register', agent);
  }

  unregister(agentId: string): boolean {
    const agent = this.agents.get(agentId);
    if (!agent) return false;
    this.agents.delete(agentId);
    this.emit('unregister', agent);
    return true;
  }

  getAgent(agentId: string): AgentInfo | undefined {
    return this.agents.get(agentId);
  }

  listAgents(): AgentInfo[] {
    return Array.from(this.agents.values());
  }

  findByCapability(capability: string): AgentInfo[] {
    return this.listAgents().filter(
      (a) => a.capabilities?.includes(capability) ?? false
    );
  }

  updateLastSeen(agentId: string): void {
    const agent = this.agents.get(agentId);
    if (agent) {
      agent.lastSeen = new Date().toISOString();
    }
  }
}
