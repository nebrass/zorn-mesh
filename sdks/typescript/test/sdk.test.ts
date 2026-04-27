import { describe, expect, test } from "bun:test";
import { SDK_BOUNDARY, createClient } from "../src/index";

describe("TypeScript SDK boundary", () => {
  test("exposes a Bun-managed zornmesh client scaffold", () => {
    const client = createClient({ agentId: "agent.local/dev" });

    expect(SDK_BOUNDARY).toBe("zornmesh-typescript-sdk");
    expect(client.agentId).toBe("agent.local/dev");
    expect(client.runtime).toBe("bun");
  });
});
