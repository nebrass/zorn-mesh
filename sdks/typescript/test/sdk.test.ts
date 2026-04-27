import { afterEach, describe, expect, test } from "bun:test";
import {
  CONNECT_STATE_NAMES,
  COORDINATION_CONTRACT_VERSION,
  COORDINATION_OUTCOME_KINDS,
  DEFAULT_CONNECT_TIMEOUT_MS,
  DEFAULT_RETRY_DELAY_MS,
  DELIVERY_STATE_TAXONOMY_VERSION,
  ENVELOPE_SCHEMA_VERSION,
  ERROR_CATEGORIES,
  ERROR_CONTRACT_VERSION,
  NACK_REASON_CATEGORIES,
  SDK_BOUNDARY,
  SDK_ERROR_CODES,
  SdkError,
  autospawnedDaemonCountForTests,
  connect,
  createClient,
  hasAutospawnedDaemonForTests,
  shutdownAutospawnedDaemonForTests,
  type DaemonStarter,
  type Envelope,
} from "../src/index";

describe("TypeScript SDK boundary", () => {
  test("exposes a Bun-managed zornmesh client scaffold without non-Bun test dependencies", async () => {
    const packageJson = await Bun.file(new URL("../package.json", import.meta.url)).json();
    const client = createClient({ agentId: "agent.local/dev" });

    expect(SDK_BOUNDARY).toBe("zornmesh-typescript-sdk");
    expect(client.agentId).toBe("agent.local/dev");
    expect(client.runtime).toBe("bun");
    expect(packageJson.packageManager).toStartWith("bun@");
    expect(packageJson.scripts.test).toBe("bun test");
    expect(JSON.stringify(packageJson).toLowerCase()).not.toMatch(
      /\b(npm|pnpm|yarn|vitest|jest|mocha)\b/,
    );
  });

  test("pins the shared coordination fixture taxonomy", async () => {
    const fixture = await Bun.file(
      new URL("../../../fixtures/coordination/contract.txt", import.meta.url),
    ).text();

    expect(COORDINATION_CONTRACT_VERSION).toBe("zornmesh.coordination.v1");
    expect(ENVELOPE_SCHEMA_VERSION).toBe("zornmesh.envelope.v1");
    expect(ERROR_CONTRACT_VERSION).toBe("zornmesh.error.v1");
    expect(DELIVERY_STATE_TAXONOMY_VERSION).toBe("zornmesh.delivery-state.v1");
    for (const kind of COORDINATION_OUTCOME_KINDS) {
      expect(fixture).toContain(`outcome|${kind}|`);
    }
    for (const reason of NACK_REASON_CATEGORIES) {
      expect(fixture).toContain(`nack_reason|${reason}`);
    }
    expect(ERROR_CATEGORIES).toContain("persistence_unavailable");
  });
});

describe("TypeScript SDK connect contract", () => {
  test("pins the shared Rust/TypeScript connect fixture taxonomy", async () => {
    const fixture = await Bun.file(
      new URL("../../../fixtures/sdk/connect-contract.json", import.meta.url),
    ).json();

    expect(CONNECT_STATE_NAMES).toEqual(fixture.state_names);
    expect(SDK_ERROR_CODES).toEqual(expect.arrayContaining(fixture.error_codes));
    expect(DEFAULT_CONNECT_TIMEOUT_MS).toBe(fixture.connect_timeout_ms);
    expect(DEFAULT_RETRY_DELAY_MS).toBe(fixture.retry_initial_delay_ms);
  });

  test("returns stable daemon-unreachable fields when auto-spawn is disabled", async () => {
    const socketPath = uniqueSocketPath("disabled");

    const error = await connect({
      agentId: "agent.local/typescript-disabled",
      socketPath,
      autoSpawn: false,
    }).catch((value: unknown) => value);

    expect(error).toBeInstanceOf(SdkError);
    expect((error as SdkError).code).toBe("E_DAEMON_UNREACHABLE");
    expect((error as SdkError).retryable).toBe(true);
    expect(await socketStat(socketPath)).toBeNull();
  });

  test("auto-spawns once for concurrent TypeScript connects", async () => {
    const socketPath = uniqueSocketPath("concurrent");
    const starter = harnessStarter();

    const clients = await Promise.all(
      Array.from({ length: 8 }, (_, index) =>
        connect({
          agentId: `agent.local/typescript-${index}`,
          socketPath,
          connectTimeoutMs: 500,
          daemonStarter: starter.start,
        }),
      ),
    );

    expect(clients.map((client) => client.socketPath)).toEqual(Array(8).fill(socketPath));
    expect(starter.starts()).toBe(1);
    expect(hasAutospawnedDaemonForTests(socketPath)).toBe(true);
    expect(autospawnedDaemonCountForTests(socketPath)).toBe(1);
  });

  test("cleans up a timed-out TypeScript auto-spawn attempt", async () => {
    const socketPath = uniqueSocketPath("timeout");
    let timer: Timer | undefined;
    const start: DaemonStarter = () => {
      timer = setTimeout(() => {
        void HarnessDaemon.start(socketPath).then((daemon) => registerCleanup(() => daemon.stop()));
      }, 50);
      return {
        shutdown: () => {
          if (timer) {
            clearTimeout(timer);
          }
        },
      };
    };

    const error = await connect({
      agentId: "agent.local/typescript-timeout",
      socketPath,
      connectTimeoutMs: 1,
      daemonStarter: start,
    }).catch((value: unknown) => value);

    expect(error).toBeInstanceOf(SdkError);
    expect((error as SdkError).code).toBe("E_DAEMON_READINESS_TIMEOUT");
    expect((error as SdkError).retryable).toBe(true);
    expect(hasAutospawnedDaemonForTests(socketPath)).toBe(false);
  });

  test("replaces a stale trusted socket during TypeScript auto-spawn", async () => {
    const socketPath = uniqueSocketPath("stale");
    await createStaleSocket(socketPath);
    expect(await socketStat(socketPath)).toMatchObject({ kind: "socket", unsafe: false });
    const starter = harnessStarter({ removeExistingSocket: true });

    const client = await connect({
      agentId: "agent.local/typescript-stale",
      socketPath,
      connectTimeoutMs: 500,
      daemonStarter: starter.start,
    });

    expect(client.socketPath).toBe(socketPath);
    expect(starter.starts()).toBe(1);
  });
});

describe("TypeScript SDK first-message pub/sub parity", () => {
  test("publishes and subscribes through the shared local frame contract", async () => {
    const socketPath = uniqueSocketPath("pubsub");
    const daemon = await HarnessDaemon.start(socketPath);
    registerCleanup(() => daemon.stop());
    const subscriber = await connect({
      agentId: "agent.local/subscriber",
      socketPath,
      autoSpawn: false,
    });
    const publisher = await connect({
      agentId: "agent.local/publisher",
      socketPath,
      autoSpawn: false,
    });
    const subscription = await subscriber.subscribe("mesh.trace.>");
    const payload = new TextEncoder().encode('{"trace_id":"trace-1"}');

    const result = await publisher.publish({
      subject: "mesh.trace.created",
      payload,
      correlation_id: "corr-typescript-1",
      timestamp_unix_ms: 1_717_000_000_000,
    });
    const delivery = await subscription.recvDelivery(500);

    expect(result.status).toBe("accepted");
    expect(result.code).toBe("ACCEPTED");
    expect(result.outcome).toMatchObject({
      kind: "accepted",
      stage: "transport",
      retryable: false,
      terminal: false,
      delivery_attempts: 1,
    });
    expect(result.durable_outcome).toMatchObject({
      kind: "failed",
      stage: "durable",
      code: "E_PERSISTENCE_UNAVAILABLE",
      retryable: false,
      terminal: true,
    });
    expect(delivery?.attempt).toBe(1);
    expect(delivery?.delivery_id).toBe("corr-typescript-1:1");
    expect(delivery?.envelope).toMatchObject({
      source_agent: "agent.local/publisher",
      subject: "mesh.trace.created",
      correlation_id: "corr-typescript-1",
      timestamp_unix_ms: 1_717_000_000_000,
      payload_metadata: {
        content_type: "application/octet-stream",
        payload_len: payload.byteLength,
      },
    });
    expect(Array.from(delivery?.envelope.payload ?? [])).toEqual(Array.from(payload));
    expect(Object.keys(delivery ?? {})).toEqual(["delivery_id", "envelope", "attempt"]);
    expect(Object.keys(delivery?.envelope ?? {})).toEqual([
      "source_agent",
      "subject",
      "timestamp_unix_ms",
      "correlation_id",
      "payload_metadata",
      "payload",
    ]);
    expect("sourceAgent" in (delivery?.envelope ?? {})).toBe(false);

    const ack = await subscription.ack(delivery!);
    expect(ack).toMatchObject({
      delivery_id: "corr-typescript-1:1",
      kind: "acknowledged",
      stage: "delivery",
      reason: null,
    });

    const nackPayload = new TextEncoder().encode("{}");
    await publisher.publish({
      subject: "mesh.trace.rejected",
      payload: nackPayload,
      correlation_id: "corr-typescript-2",
      timestamp_unix_ms: 1_717_000_000_001,
    });
    const rejectedDelivery = await subscription.recvDelivery(500);
    const nack = await subscription.nack(rejectedDelivery!, "processing");
    expect(nack).toMatchObject({
      delivery_id: "corr-typescript-2:1",
      kind: "rejected",
      stage: "delivery",
      reason: "processing",
    });

    subscription.close();
  });
});

type Cleanup = () => void | Promise<void>;
type HarnessSocket = {
  write(data: Uint8Array): number;
  end(): void;
  data?: {
    buffer?: Uint8Array;
    pattern?: string;
  };
};

const cleanupStack: Cleanup[] = [];
const runtimeDirs = new Set<string>();
let sequence = 0;

afterEach(async () => {
  while (cleanupStack.length > 0) {
    await cleanupStack.pop()?.();
  }
  for (const dir of runtimeDirs) {
    await run(["rm", "-rf", dir]);
  }
  runtimeDirs.clear();
});

function registerCleanup(cleanup: Cleanup): void {
  cleanupStack.push(cleanup);
}

function uniqueSocketPath(name: string): string {
  sequence += 1;
  const dir = `/tmp/zornmesh-typescript-sdk-${name}-${process.pid}-${Date.now()}-${sequence}`;
  runtimeDirs.add(dir);
  return `${dir}/zorn.sock`;
}

function socketDir(socketPath: string): string {
  return socketPath.slice(0, socketPath.lastIndexOf("/"));
}

async function prepareSocketParent(socketPath: string): Promise<void> {
  const dir = socketDir(socketPath);
  await run(["mkdir", "-p", dir]);
  await run(["chmod", "700", dir]);
}

async function socketStat(socketPath: string): Promise<{ kind: string; unsafe: boolean } | null> {
  try {
    const stat = await Bun.file(socketPath).stat();
    return {
      kind: (stat.mode & 0o170000) === 0o140000 ? "socket" : "other",
      unsafe: (stat.mode & 0o077) !== 0,
    };
  } catch {
    return null;
  }
}

async function run(command: string[]): Promise<void> {
  const result = Bun.spawn(command, { stdout: "ignore", stderr: "pipe" });
  const exitCode = await result.exited;
  if (exitCode !== 0) {
    throw new Error(`${command.join(" ")} failed: ${await new Response(result.stderr).text()}`);
  }
}

function harnessStarter(options: { removeExistingSocket?: boolean } = {}): {
  start: DaemonStarter;
  starts: () => number;
} {
  let starts = 0;
  return {
    starts: () => starts,
    start: async ({ socketPath }) => {
      starts += 1;
      if (options.removeExistingSocket) {
        await run(["rm", "-f", socketPath]);
      }
      const daemon = await HarnessDaemon.start(socketPath);
      return {
        shutdown: () => daemon.stop(),
      };
    },
  };
}

async function createStaleSocket(socketPath: string): Promise<void> {
  await prepareSocketParent(socketPath);
  const child = Bun.spawn(
    [
      "bun",
      "-e",
      `
        const server = Bun.listen({ unix: ${JSON.stringify(socketPath)}, socket: { data() {} } });
        await Bun.spawn(["chmod", "600", ${JSON.stringify(socketPath)}]).exited;
        console.log("ready");
        setInterval(() => {}, 1000);
      `,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const reader = child.stdout.getReader();
  const readyChunk = await reader.read();
  await reader.cancel();
  const ready = new TextDecoder().decode(readyChunk.value);
  expect(ready).toContain("ready");
  child.kill("SIGKILL");
  await child.exited;
  await waitForSocketFile(socketPath);
}

async function waitForSocketFile(socketPath: string): Promise<void> {
  const startedAt = performance.now();
  while (performance.now() - startedAt < 200) {
    if (await socketStat(socketPath)) {
      return;
    }
    await Bun.sleep(5);
  }
}

class HarnessDaemon {
  private readonly subscriptions = new Set<HarnessSocket>();
  private readonly server: ReturnType<typeof Bun.listen>;

  private constructor(server: ReturnType<typeof Bun.listen>) {
    this.server = server;
  }

  static async start(socketPath: string): Promise<HarnessDaemon> {
    await prepareSocketParent(socketPath);
    let daemon: HarnessDaemon;
    const server = Bun.listen({
      unix: socketPath,
      socket: {
        data(socket: HarnessSocket, chunk: Uint8Array) {
          daemon.receive(socket, chunk);
        },
        close(socket: HarnessSocket) {
          daemon.subscriptions.delete(socket);
        },
      },
    });
    daemon = new HarnessDaemon(server);
    await run(["chmod", "600", socketPath]);
    return daemon;
  }

  stop(): void {
    this.server.stop(true);
  }

  private receive(socket: HarnessSocket, chunk: Uint8Array): void {
    const data = appendBytes(socket.data?.buffer, chunk);
    socket.data = { ...socket.data, buffer: data };
    for (;;) {
      const frame = takeFrame(socket.data.buffer);
      if (!frame) {
        return;
      }
      socket.data.buffer = frame.remaining;
      const clientFrame = decodeClientFrame(frame.body);
      if (clientFrame.kind === "subscribe") {
        socket.data.pattern = clientFrame.pattern;
        this.subscriptions.add(socket);
        socket.write(encodeSendResult("accepted", "ACCEPTED", "subscription accepted"));
        continue;
      }

      let deliveries = 0;
      for (const subscriber of this.subscriptions) {
        const pattern = subscriber.data?.pattern;
        if (pattern && subjectMatches(pattern, clientFrame.envelope.subject)) {
          subscriber.write(encodeDelivery(clientFrame.envelope, 1));
          deliveries += 1;
        }
      }
      socket.write(
        encodeSendResult(
          "accepted",
          "ACCEPTED",
          `accepted for routing; delivery_attempts=${deliveries}`,
        ),
      );
      socket.end();
    }
  }
}

type ClientFrame =
  | { kind: "subscribe"; pattern: string }
  | { kind: "publish"; envelope: Envelope };

function appendBytes(left: Uint8Array | undefined, right: Uint8Array): Uint8Array {
  if (!left || left.byteLength === 0) {
    return new Uint8Array(right);
  }
  const output = new Uint8Array(left.byteLength + right.byteLength);
  output.set(left, 0);
  output.set(right, left.byteLength);
  return output;
}

function takeFrame(buffer: Uint8Array | undefined): { body: Uint8Array; remaining: Uint8Array } | null {
  if (!buffer || buffer.byteLength < 4) {
    return null;
  }
  const length = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength).getUint32(0);
  if (buffer.byteLength < 4 + length) {
    return null;
  }
  return {
    body: buffer.slice(4, 4 + length),
    remaining: buffer.slice(4 + length),
  };
}

function decodeClientFrame(body: Uint8Array): ClientFrame {
  const cursor = new Cursor(body);
  cursor.expectMagic();
  cursor.expectVersion();
  const kind = cursor.u8();
  if (kind === 1) {
    const pattern = cursor.string();
    cursor.expectEnd();
    return { kind: "subscribe", pattern };
  }
  if (kind === 2) {
    const envelope = cursor.envelope();
    cursor.expectEnd();
    return { kind: "publish", envelope };
  }
  throw new Error(`unexpected client frame kind ${kind}`);
}

function encodeSendResult(status: "accepted" | "rejected" | "validation_failed", code: string, message: string): Uint8Array {
  const body = frameBody(101);
  body.push(status === "accepted" ? 1 : status === "rejected" ? 2 : 3);
  putString(body, code);
  putString(body, message);
  return withLength(body);
}

function encodeDelivery(envelope: Envelope, attempt: number): Uint8Array {
  const body = frameBody(102);
  putU32(body, attempt);
  putEnvelope(body, envelope);
  return withLength(body);
}

function frameBody(kind: number): number[] {
  return [0x5a, 0x4d, 0x00, 0x01, kind];
}

function withLength(body: number[]): Uint8Array {
  const output = new Uint8Array(4 + body.length);
  new DataView(output.buffer).setUint32(0, body.length);
  output.set(body, 4);
  return output;
}

function putEnvelope(output: number[], envelope: Envelope): void {
  putString(output, envelope.source_agent);
  putString(output, envelope.subject);
  putU64(output, envelope.timestamp_unix_ms);
  putString(output, envelope.correlation_id);
  putString(output, envelope.payload_metadata.content_type);
  putBytes(output, envelope.payload);
}

function putString(output: number[], value: string): void {
  putBytes(output, new TextEncoder().encode(value));
}

function putBytes(output: number[], value: Uint8Array): void {
  putU32(output, value.byteLength);
  output.push(...value);
}

function putU32(output: number[], value: number): void {
  output.push((value >>> 24) & 0xff, (value >>> 16) & 0xff, (value >>> 8) & 0xff, value & 0xff);
}

function putU64(output: number[], value: number): void {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value));
  output.push(...bytes);
}

function subjectMatches(pattern: string, subject: string): boolean {
  const patternLevels = pattern.split(".");
  const subjectLevels = subject.split(".");
  for (let index = 0; index < patternLevels.length; index += 1) {
    const patternLevel = patternLevels[index];
    if (patternLevel === ">") {
      return index === patternLevels.length - 1;
    }
    if (patternLevel !== "*" && patternLevel !== subjectLevels[index]) {
      return false;
    }
  }
  return patternLevels.length === subjectLevels.length;
}

class Cursor {
  private offset = 0;

  constructor(private readonly bytes: Uint8Array) {}

  expectMagic(): void {
    expect(this.take(2)).toEqual(new Uint8Array([0x5a, 0x4d]));
  }

  expectVersion(): void {
    expect(this.u16()).toBe(1);
  }

  expectEnd(): void {
    expect(this.offset).toBe(this.bytes.byteLength);
  }

  envelope(): Envelope {
    const source_agent = this.string();
    const subject = this.string();
    const timestamp_unix_ms = this.u64();
    const correlation_id = this.string();
    const content_type = this.string();
    const payload = this.bytesField();
    return {
      source_agent,
      subject,
      timestamp_unix_ms,
      correlation_id,
      payload_metadata: {
        content_type,
        payload_len: payload.byteLength,
      },
      payload,
    };
  }

  u8(): number {
    return this.take(1)[0];
  }

  u16(): number {
    const bytes = this.take(2);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint16(0);
  }

  u32(): number {
    const bytes = this.take(4);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0);
  }

  u64(): number {
    const bytes = this.take(8);
    return Number(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0));
  }

  string(): string {
    return new TextDecoder().decode(this.bytesField());
  }

  bytesField(): Uint8Array {
    return this.take(this.u32());
  }

  private take(length: number): Uint8Array {
    const end = this.offset + length;
    expect(end).toBeLessThanOrEqual(this.bytes.byteLength);
    const value = this.bytes.slice(this.offset, end);
    this.offset = end;
    return value;
  }
}
