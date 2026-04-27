export const SDK_BOUNDARY = "zornmesh-typescript-sdk" as const;

export interface ClientOptions {
  agentId: string;
}

export interface ZornMeshClient {
  agentId: string;
  runtime: "bun";
}

export function createClient(options: ClientOptions): ZornMeshClient {
  const agentId = options.agentId.trim();
  if (agentId.length === 0) {
    throw new Error("agentId is required");
  }

  return {
    agentId,
    runtime: "bun",
  };
}
