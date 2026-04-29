export const SDK_BOUNDARY = "zornmesh-typescript-sdk" as const;
export const CONNECT_STATE_NAMES = ["ready", "draining"] as const;
export const COORDINATION_CONTRACT_VERSION = "zornmesh.coordination.v1" as const;
export const ENVELOPE_SCHEMA_VERSION = "zornmesh.envelope.v1" as const;
export const ERROR_CONTRACT_VERSION = "zornmesh.error.v1" as const;
export const DELIVERY_STATE_TAXONOMY_VERSION = "zornmesh.delivery-state.v1" as const;
export const COORDINATION_OUTCOME_KINDS = [
  "accepted",
  "durable_accepted",
  "acknowledged",
  "rejected",
  "failed",
  "timed_out",
  "retryable",
  "terminal",
] as const;
export const COORDINATION_STAGES = ["transport", "durable", "delivery", "protocol"] as const;
export const NACK_REASON_CATEGORIES = [
  "validation",
  "authorization",
  "processing",
  "timeout",
  "payload_limit",
  "backpressure",
  "transient",
  "policy",
  "unknown",
] as const;
export const ERROR_CATEGORIES = [
  "validation",
  "authorization",
  "reachability",
  "timeout",
  "payload_limit",
  "protocol",
  "persistence_unavailable",
  "conflict",
  "internal",
] as const;
export const DEFAULT_CONNECT_TIMEOUT_MS = 1_000;
export const DEFAULT_RETRY_DELAY_MS = 5;
export const SDK_ERROR_CODES = [
  "E_LOCAL_TRUST_UNSAFE",
  "E_ELEVATED_PRIVILEGE",
  "E_DAEMON_UNREACHABLE",
  "E_DAEMON_READINESS_TIMEOUT",
  "E_INVALID_CONFIG",
  "E_SUBJECT_VALIDATION",
  "E_SUBSCRIPTION_CAP",
  "E_PAYLOAD_LIMIT",
  "E_PROTOCOL",
  "E_PERSISTENCE_UNAVAILABLE",
  "E_DELIVERY_VALIDATION",
  "E_DAEMON_IO",
] as const;

const MAGIC = new Uint8Array([0x5a, 0x4d]);
const ENVELOPE_WIRE_VERSION = 1;
const MAX_FRAME_BYTES = 64 * 1024;
const MAX_SUBJECT_BYTES = 256;
const MAX_SUBJECT_LEVELS = 8;
const CLIENT_SUBSCRIBE = 1;
const CLIENT_PUBLISH = 2;
const CLIENT_ACK = 3;
const CLIENT_NACK = 4;
const SERVER_SEND_RESULT = 101;
const SERVER_DELIVERY = 102;
const SERVER_DELIVERY_OUTCOME = 103;
const ENV_SOCKET_PATH = "ZORN_SOCKET_PATH";
const ENV_NO_AUTOSPAWN = "ZORN_NO_AUTOSPAWN";
const SOCKET_KIND = 0o140000;
const FILE_KIND_MASK = 0o170000;

export type SdkErrorCode = (typeof SDK_ERROR_CODES)[number];
export type ConnectStateName = (typeof CONNECT_STATE_NAMES)[number];
export type SendStatus = "accepted" | "rejected" | "daemon_unreachable" | "validation_failed";
export type CoordinationOutcomeKind = (typeof COORDINATION_OUTCOME_KINDS)[number];
export type CoordinationStage = (typeof COORDINATION_STAGES)[number];
export type NackReasonCategory = (typeof NACK_REASON_CATEGORIES)[number];
export type ErrorCategory = (typeof ERROR_CATEGORIES)[number];

export interface ClientOptions {
  agentId: string;
}

export interface ConnectOptions extends ClientOptions {
  socketPath?: string;
  autoSpawn?: boolean;
  connectTimeoutMs?: number;
  retryDelayMs?: number;
  daemonCommand?: readonly string[];
  daemonStarter?: DaemonStarter;
}

export interface DaemonStarterContext {
  socketPath: string;
  command: readonly string[];
  env: Record<string, string>;
}

export interface StartedDaemon {
  shutdown(): void | Promise<void>;
}

export type DaemonStarter = (
  context: DaemonStarterContext,
) => void | StartedDaemon | Promise<void | StartedDaemon>;

export interface ZornMeshClient {
  agentId: string;
  runtime: "bun";
}

export interface ConnectedZornMeshClient extends ZornMeshClient {
  socketPath: string;
  /**
   * Publishes one zornmesh envelope through the connected daemon.
   *
   * @example
   * ```ts
   * const result = await mesh.publish({
   *   subject: "mesh.trace.created",
   *   payload: JSON.stringify({ trace_id: "trace-1" }),
   * });
   * ```
   */
  publish(input: PublishInput): Promise<SendResult>;
  /**
   * Subscribes to an exact, prefix (`>`) or single-level wildcard (`*`) zornmesh subject pattern.
   *
   * @example
   * ```ts
   * const subscription = await mesh.subscribe("mesh.trace.>");
   * const delivery = await subscription.recvDelivery(500);
   * ```
   */
  subscribe(pattern: string): Promise<Subscription>;
}

export interface PayloadMetadata {
  content_type: string;
  payload_len: number;
}

export interface Envelope {
  source_agent: string;
  subject: string;
  timestamp_unix_ms: number;
  correlation_id: string;
  payload_metadata: PayloadMetadata;
  payload: Uint8Array;
}

export interface PublishInput {
  source_agent?: string;
  subject: string;
  payload?: Uint8Array | ArrayBuffer | string;
  timestamp_unix_ms?: number;
  correlation_id?: string;
  content_type?: string;
}

export interface SendResult {
  status: SendStatus;
  code: string;
  message: string;
  outcome: CoordinationOutcome;
  durable_outcome: CoordinationOutcome | null;
}

export interface Delivery {
  delivery_id: string;
  envelope: Envelope;
  attempt: number;
}

export interface CoordinationOutcome {
  version: typeof COORDINATION_CONTRACT_VERSION;
  kind: CoordinationOutcomeKind;
  stage: CoordinationStage;
  code: string;
  message: string;
  retryable: boolean;
  terminal: boolean;
  delivery_attempts: number;
}

export interface DeliveryOutcome {
  delivery_id: string;
  kind: CoordinationOutcomeKind;
  stage: CoordinationStage;
  code: string;
  message: string;
  retryable: boolean;
  terminal: boolean;
  reason: NackReasonCategory | null;
  outcome: CoordinationOutcome;
}

interface NormalizedConnectOptions {
  agentId: string;
  socketPath: string;
  autoSpawn: boolean;
  connectTimeoutMs: number;
  retryDelayMs: number;
  daemonCommand: readonly string[];
  daemonStarter: DaemonStarter;
}

interface ManagedDaemon {
  shutdown(): void | Promise<void>;
}

type BunSocket = {
  write(data: Uint8Array): number;
  end(): void;
};

type ServerFrame =
  | { kind: "send_result"; result: SendResult }
  | { kind: "delivery"; delivery_id: string; envelope: Envelope; attempt: number }
  | { kind: "delivery_outcome"; outcome: DeliveryOutcome };

const managedDaemons = new Map<string, ManagedDaemon>();
const daemonStartPromises = new Map<string, Promise<boolean>>();

export class SdkError extends Error {
  readonly code: SdkErrorCode;
  readonly retryable: boolean;
  readonly category: ErrorCategory;
  readonly safe_details: string;

  constructor(code: SdkErrorCode, message: string, retryable = isRetryableCode(code)) {
    super(`${code}: ${message}`);
    this.name = "ZornMeshSdkError";
    this.code = code;
    this.retryable = retryable;
    this.category = errorCategoryForCode(code);
    this.safe_details = message;
  }
}

export function createClient(options: ClientOptions): ZornMeshClient {
  const agentId = options.agentId.trim();
  if (agentId.length === 0) {
    throw new SdkError("E_INVALID_CONFIG", "agentId is required", false);
  }

  return {
    agentId,
    runtime: "bun",
  };
}

/**
 * Connects a Bun TypeScript agent to the local zornmesh daemon, auto-spawning
 * the daemon unless disabled by options or `ZORN_NO_AUTOSPAWN=1`.
 *
 * @example
 * ```ts
 * import { connect } from "@zornmesh/sdk";
 *
 * const mesh = await connect({ agentId: "agent.local/typescript" });
 * const subscription = await mesh.subscribe("mesh.trace.>");
 * await mesh.publish({ subject: "mesh.trace.created", payload: "{}" });
 * const delivery = await subscription.recvDelivery(500);
 * ```
 */
export async function connect(options: ConnectOptions): Promise<ConnectedZornMeshClient> {
  const normalized = await normalizeConnectOptions(options);
  const uid = effectiveUid();

  try {
    await probeTrustedSocket(normalized.socketPath, uid);
    return connectedClient(normalized.agentId, normalized.socketPath);
  } catch (error) {
    const sdkError = asSdkError(error);
    if (sdkError.code !== "E_DAEMON_UNREACHABLE") {
      throw sdkError;
    }
    if (!normalized.autoSpawn) {
      throw daemonUnreachableAutospawnDisabled();
    }
  }

  const sdkStartedDaemon = await ensureDaemonStarted(normalized);
  try {
    await waitForReadiness(normalized, uid);
  } catch (error) {
    const sdkError = asSdkError(error);
    if (sdkStartedDaemon && sdkError.code === "E_DAEMON_READINESS_TIMEOUT") {
      await shutdownAutospawnedDaemonForTests(normalized.socketPath);
    }
    throw sdkError;
  }

  return connectedClient(normalized.agentId, normalized.socketPath);
}

export function hasAutospawnedDaemonForTests(socketPath: string): boolean {
  return managedDaemons.has(socketPath);
}

export function autospawnedDaemonCountForTests(socketPath: string): number {
  return managedDaemons.has(socketPath) ? 1 : 0;
}

export async function shutdownAutospawnedDaemonForTests(socketPath: string): Promise<void> {
  const daemon = managedDaemons.get(socketPath);
  managedDaemons.delete(socketPath);
  if (daemon) {
    await daemon.shutdown();
  }
}

function connectedClient(agentId: string, socketPath: string): ConnectedZornMeshClient {
  return {
    agentId,
    runtime: "bun",
    socketPath,
    publish(input: PublishInput) {
      return publish(socketPath, agentId, input);
    },
    subscribe(pattern: string) {
      return subscribe(socketPath, pattern);
    },
  };
}

async function publish(socketPath: string, agentId: string, input: PublishInput): Promise<SendResult> {
  let envelope: Envelope;
  try {
    envelope = envelopeFromInput(agentId, input);
  } catch (error) {
    return sendResultFromError(asSdkError(error));
  }

  let connection: FrameConnection;
  try {
    connection = await FrameConnection.open(socketPath);
  } catch (error) {
    const sdkError = asSdkError(error);
    return sendResultFromError(sdkError);
  }

  try {
    connection.write(encodeClientFrame({ kind: "publish", envelope }));
    const frame = await connection.nextFrame(DEFAULT_CONNECT_TIMEOUT_MS);
    if (!frame) {
      throw new SdkError("E_PROTOCOL", "daemon closed connection before send result", false);
    }
    if (frame.kind === "send_result") {
      return frame.result;
    }
    return sendResultFromError(
      new SdkError("E_PROTOCOL", "E_PROTOCOL: daemon returned delivery to publisher", false),
    );
  } catch (error) {
    return sendResultFromError(asSdkError(error));
  } finally {
    connection.close();
  }
}

async function subscribe(socketPath: string, pattern: string): Promise<Subscription> {
  const connection = await FrameConnection.open(socketPath);
  try {
    connection.write(encodeClientFrame({ kind: "subscribe", pattern }));
    const frame = await connection.nextFrame(DEFAULT_CONNECT_TIMEOUT_MS);
    if (!frame) {
      throw new SdkError("E_PROTOCOL", "daemon closed connection before subscription acceptance", false);
    }
    if (frame.kind !== "send_result") {
      throw new SdkError(
        "E_PROTOCOL",
        "daemon returned delivery before subscription acceptance",
        false,
      );
    }
    if (frame.result.status !== "accepted") {
      throw sdkErrorFromSendResult(frame.result);
    }
    return new Subscription(connection);
  } catch (error) {
    connection.close();
    throw asSdkError(error);
  }
}

export class Subscription {
  constructor(private readonly connection: FrameConnection) {}

  /**
   * Waits for the next matching zornmesh delivery from this subscription.
   * Returns `null` when no delivery arrives before `timeoutMs`.
   */
  async recvDelivery(timeoutMs: number): Promise<Delivery | null> {
    const frame = await this.connection.nextFrame(timeoutMs).catch((error: unknown) => {
      const sdkError = asSdkError(error);
      if (sdkError.code === "E_DAEMON_IO" && sdkError.message.includes("timed out")) {
        return null;
      }
      throw sdkError;
    });
    if (!frame) {
      return null;
    }
    if (frame.kind === "send_result") {
      throw sdkErrorFromSendResult(frame.result);
    }
    if (frame.kind === "delivery_outcome") {
      throw sdkErrorFromDeliveryOutcome(frame.outcome);
    }
    return {
      delivery_id: frame.delivery_id,
      envelope: frame.envelope,
      attempt: frame.attempt,
    };
  }

  async ack(delivery: Delivery | string): Promise<DeliveryOutcome> {
    return this.deliveryControl({
      kind: "ack",
      delivery_id: typeof delivery === "string" ? delivery : delivery.delivery_id,
    });
  }

  async nack(delivery: Delivery | string, reason: NackReasonCategory): Promise<DeliveryOutcome> {
    return this.deliveryControl({
      kind: "nack",
      delivery_id: typeof delivery === "string" ? delivery : delivery.delivery_id,
      reason,
    });
  }

  private async deliveryControl(frame: ClientFrame): Promise<DeliveryOutcome> {
    this.connection.write(encodeClientFrame(frame));
    const response = await this.connection.nextFrame(DEFAULT_CONNECT_TIMEOUT_MS);
    if (!response) {
      throw new SdkError("E_PROTOCOL", "daemon closed connection before ACK/NACK outcome", false);
    }
    if (response.kind === "delivery_outcome") {
      return response.outcome;
    }
    if (response.kind === "send_result") {
      throw sdkErrorFromSendResult(response.result);
    }
    throw new SdkError("E_PROTOCOL", "daemon returned delivery before ACK/NACK outcome", false);
  }

  close(): void {
    this.connection.close();
  }
}

class FrameConnection {
  private socket: BunSocket | undefined;
  private buffer = new Uint8Array();
  private readonly frames: ServerFrame[] = [];
  private readonly waiters: Array<{
    resolve(frame: ServerFrame | null): void;
    reject(error: SdkError): void;
    timer: Timer;
  }> = [];
  private closed = false;

  static async open(socketPath: string): Promise<FrameConnection> {
    await validateSocketTrust(socketPath, effectiveUid());
    const connection = new FrameConnection();
    try {
      connection.socket = await Bun.connect({
        unix: socketPath,
        socket: {
          data(_socket: BunSocket, chunk: Uint8Array) {
            connection.receive(chunk);
          },
          close() {
            connection.markClosed();
          },
          error(_socket: BunSocket, error: Error) {
            connection.fail(new SdkError("E_DAEMON_IO", error.message));
          },
        },
      });
    } catch (error) {
      throw daemonUnreachableFromConnect(error);
    }
    return connection;
  }

  write(frame: Uint8Array): void {
    if (!this.socket || this.closed) {
      throw new SdkError("E_DAEMON_UNREACHABLE", "daemon connection is closed");
    }
    this.socket.write(frame);
  }

  nextFrame(timeoutMs: number): Promise<ServerFrame | null> {
    const frame = this.frames.shift();
    if (frame) {
      return Promise.resolve(frame);
    }
    if (this.closed) {
      return Promise.resolve(null);
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const index = this.waiters.findIndex((waiter) => waiter.timer === timer);
        if (index >= 0) {
          this.waiters.splice(index, 1);
        }
        reject(new SdkError("E_DAEMON_IO", "timed out waiting for daemon frame"));
      }, timeoutMs);
      this.waiters.push({ resolve, reject, timer });
    });
  }

  close(): void {
    this.closed = true;
    this.socket?.end();
    while (this.waiters.length > 0) {
      const waiter = this.waiters.shift();
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.resolve(null);
      }
    }
  }

  private receive(chunk: Uint8Array): void {
    this.buffer = appendBytes(this.buffer, chunk);
    for (;;) {
      const frame = takeFrame(this.buffer);
      if (!frame) {
        return;
      }
      this.buffer = frame.remaining;
      let serverFrame: ServerFrame;
      try {
        serverFrame = decodeServerFrame(frame.body);
      } catch (error) {
        this.fail(asSdkError(error));
        return;
      }
      const waiter = this.waiters.shift();
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.resolve(serverFrame);
      } else {
        this.frames.push(serverFrame);
      }
    }
  }

  private markClosed(): void {
    this.closed = true;
    while (this.waiters.length > 0) {
      const waiter = this.waiters.shift();
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.resolve(null);
      }
    }
  }

  private fail(error: SdkError): void {
    this.closed = true;
    while (this.waiters.length > 0) {
      const waiter = this.waiters.shift();
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.reject(error);
      }
    }
  }
}

async function normalizeConnectOptions(options: ConnectOptions): Promise<NormalizedConnectOptions> {
  const client = createClient(options);
  const socketPath = options.socketPath ?? resolveSocketPathFromEnv();
  if (socketPath.length === 0) {
    throw new SdkError("E_INVALID_CONFIG", "ZORN_SOCKET_PATH must not be empty", false);
  }
  const autoSpawn = options.autoSpawn ?? autoSpawnFromEnv(process.env[ENV_NO_AUTOSPAWN]);
  const connectTimeoutMs = positiveMillis(
    options.connectTimeoutMs ?? DEFAULT_CONNECT_TIMEOUT_MS,
    "connectTimeoutMs",
  );
  const retryDelayMs = positiveMillis(options.retryDelayMs ?? DEFAULT_RETRY_DELAY_MS, "retryDelayMs");
  const daemonCommand = options.daemonCommand ?? defaultDaemonCommand(socketPath);
  return {
    agentId: client.agentId,
    socketPath,
    autoSpawn,
    connectTimeoutMs,
    retryDelayMs,
    daemonCommand,
    daemonStarter: options.daemonStarter ?? defaultDaemonStarter,
  };
}

async function ensureDaemonStarted(options: NormalizedConnectOptions): Promise<boolean> {
  if (managedDaemons.has(options.socketPath)) {
    return false;
  }

  const existing = daemonStartPromises.get(options.socketPath);
  if (existing) {
    return existing;
  }

  const started = Promise.resolve()
    .then(async () => {
      const daemon = await options.daemonStarter({
        socketPath: options.socketPath,
        command: options.daemonCommand,
        env: { ...process.env, [ENV_SOCKET_PATH]: options.socketPath },
      });
      managedDaemons.set(options.socketPath, normalizeManagedDaemon(daemon));
      return true;
    })
    .finally(() => {
      daemonStartPromises.delete(options.socketPath);
    });

  daemonStartPromises.set(options.socketPath, started);
  return started;
}

function normalizeManagedDaemon(daemon: void | StartedDaemon): ManagedDaemon {
  if (daemon) {
    return daemon;
  }
  return {
    shutdown() {},
  };
}

function defaultDaemonStarter(context: DaemonStarterContext): StartedDaemon {
  const process = Bun.spawn(context.command, {
    stdout: "ignore",
    stderr: "ignore",
    env: context.env,
  });
  return {
    shutdown() {
      process.kill("SIGTERM");
    },
  };
}

function defaultDaemonCommand(socketPath: string): readonly string[] {
  return ["cargo", "run", "-q", "-p", "zornmesh", "--", "daemon", "--socket", socketPath];
}

async function waitForReadiness(options: NormalizedConnectOptions, uid: number | undefined): Promise<void> {
  const startedAt = performance.now();
  let lastError: SdkError | undefined;

  for (;;) {
    try {
      await probeTrustedSocket(options.socketPath, uid);
      return;
    } catch (error) {
      const sdkError = asSdkError(error);
      if (sdkError.code !== "E_DAEMON_UNREACHABLE") {
        throw sdkError;
      }
      lastError = sdkError;
    }

    const elapsed = performance.now() - startedAt;
    if (elapsed >= options.connectTimeoutMs) {
      throw readinessTimeout(options.connectTimeoutMs, lastError);
    }
    await sleep(Math.min(options.retryDelayMs, options.connectTimeoutMs - elapsed));
  }
}

async function probeTrustedSocket(socketPath: string, uid: number | undefined): Promise<void> {
  await validateSocketTrust(socketPath, uid);
  let socket: BunSocket | undefined;
  try {
    socket = await Bun.connect({
      unix: socketPath,
      socket: {
        data() {},
      },
    });
  } catch (error) {
    throw daemonUnreachableFromConnect(error);
  } finally {
    socket?.end();
  }
}

async function validateSocketTrust(socketPath: string, uid: number | undefined): Promise<void> {
  let stat: Awaited<ReturnType<ReturnType<typeof Bun.file>["stat"]>>;
  try {
    stat = await Bun.file(socketPath).stat();
  } catch (error) {
    throw new SdkError(
      "E_DAEMON_UNREACHABLE",
      `daemon socket is not reachable: ${errorMessage(error)}`,
    );
  }

  if ((stat.mode & FILE_KIND_MASK) !== SOCKET_KIND) {
    throw new SdkError(
      "E_LOCAL_TRUST_UNSAFE",
      "local daemon endpoint must be a Unix-domain socket owned by the current user",
      false,
    );
  }
  if (uid !== undefined && stat.uid !== uid) {
    throw new SdkError(
      "E_LOCAL_TRUST_UNSAFE",
      "local daemon socket ownership does not match the current user",
      false,
    );
  }
  if ((stat.mode & 0o077) !== 0) {
    throw new SdkError(
      "E_LOCAL_TRUST_UNSAFE",
      "local daemon socket must not be accessible by group or other users",
      false,
    );
  }
}

function envelopeFromInput(agentId: string, input: PublishInput): Envelope {
  const sourceAgent = (input.source_agent ?? agentId).trim();
  if (sourceAgent.length === 0) {
    throw new SdkError("E_INVALID_CONFIG", "envelope source agent must not be empty", false);
  }
  validateSubject(input.subject, false);
  const payload = payloadBytes(input.payload ?? new Uint8Array());
  if (payload.byteLength > MAX_FRAME_BYTES) {
    throw new SdkError(
      "E_PAYLOAD_LIMIT",
      `envelope payload is ${payload.byteLength} bytes; maximum is ${MAX_FRAME_BYTES} bytes`,
      false,
    );
  }
  const contentType = (input.content_type ?? "application/octet-stream").trim();
  if (contentType.length === 0) {
    throw new SdkError("E_INVALID_CONFIG", "envelope payload content type must not be empty", false);
  }
  const timestamp = input.timestamp_unix_ms ?? Date.now();
  const correlationId = (input.correlation_id ?? `corr-${timestamp}-${nextCorrelationId()}`).trim();
  if (correlationId.length === 0) {
    throw new SdkError("E_INVALID_CONFIG", "envelope correlation ID must not be empty", false);
  }

  return {
    source_agent: sourceAgent,
    subject: input.subject,
    timestamp_unix_ms: timestamp,
    correlation_id: correlationId,
    payload_metadata: {
      content_type: contentType,
      payload_len: payload.byteLength,
    },
    payload,
  };
}

let correlationSequence = 0;

function nextCorrelationId(): number {
  correlationSequence += 1;
  return correlationSequence;
}

function validateSubject(value: string, allowWildcards: boolean): void {
  if (value.trim().length === 0) {
    throw new SdkError("E_SUBJECT_VALIDATION", "subject must not be empty", false);
  }
  if (new TextEncoder().encode(value).byteLength > MAX_SUBJECT_BYTES) {
    throw new SdkError(
      "E_SUBJECT_VALIDATION",
      `subject is too long; maximum is ${MAX_SUBJECT_BYTES} bytes`,
      false,
    );
  }
  if (value === "zorn" || value.startsWith("zorn.")) {
    throw new SdkError("E_SUBJECT_VALIDATION", "subject must not use reserved zorn prefixes", false);
  }
  const levels = value.split(".");
  if (levels.length > MAX_SUBJECT_LEVELS) {
    throw new SdkError(
      "E_SUBJECT_VALIDATION",
      `subject has too many levels; maximum is ${MAX_SUBJECT_LEVELS}`,
      false,
    );
  }
  for (let index = 0; index < levels.length; index += 1) {
    const level = levels[index];
    if (level.length === 0) {
      throw new SdkError("E_SUBJECT_VALIDATION", "subject levels must not be empty", false);
    }
    const containsWildcard = level.includes("*") || level.includes(">");
    if (!allowWildcards && containsWildcard) {
      throw new SdkError(
        "E_SUBJECT_VALIDATION",
        "subject wildcard syntax is invalid for this operation",
        false,
      );
    }
    if (allowWildcards && containsWildcard && level !== "*" && !(level === ">" && index + 1 === levels.length)) {
      throw new SdkError(
        "E_SUBJECT_VALIDATION",
        "subject wildcard syntax is invalid for this operation",
        false,
      );
    }
    if (allowWildcards && level === ">" && index + 1 !== levels.length) {
      throw new SdkError(
        "E_SUBJECT_VALIDATION",
        "subject wildcard syntax is invalid for this operation",
        false,
      );
    }
  }
}

type ClientFrame =
  | { kind: "subscribe"; pattern: string }
  | { kind: "publish"; envelope: Envelope }
  | { kind: "ack"; delivery_id: string }
  | { kind: "nack"; delivery_id: string; reason: NackReasonCategory };

function encodeClientFrame(frame: ClientFrame): Uint8Array {
  const body = frameBody(clientFrameKind(frame));
  if (frame.kind === "subscribe") {
    validateSubject(frame.pattern, true);
    putString(body, frame.pattern);
  } else if (frame.kind === "publish") {
    putEnvelope(body, frame.envelope);
  } else if (frame.kind === "ack") {
    putString(body, frame.delivery_id);
  } else {
    putString(body, frame.delivery_id);
    putString(body, frame.reason);
  }
  if (body.length > MAX_FRAME_BYTES) {
    throw new SdkError(
      "E_PAYLOAD_LIMIT",
      `zornmesh frame is ${body.length} bytes; maximum is ${MAX_FRAME_BYTES} bytes`,
      false,
    );
  }
  return withLength(body);
}

function clientFrameKind(frame: ClientFrame): number {
  if (frame.kind === "subscribe") {
    return CLIENT_SUBSCRIBE;
  }
  if (frame.kind === "publish") {
    return CLIENT_PUBLISH;
  }
  if (frame.kind === "ack") {
    return CLIENT_ACK;
  }
  return CLIENT_NACK;
}

function decodeServerFrame(body: Uint8Array): ServerFrame {
  const cursor = new Cursor(body);
  cursor.expectMagic();
  cursor.expectVersion();
  const kind = cursor.u8("frame_type");
  if (kind === SERVER_SEND_RESULT) {
    const status = cursor.u8("status");
    const result: SendResult = {
      status: frameStatus(status),
      code: cursor.string("code"),
      message: cursor.string("message"),
      outcome: cursor.outcome(),
      durable_outcome: cursor.optionalOutcome(),
    };
    cursor.expectEnd();
    return { kind: "send_result", result };
  }
  if (kind === SERVER_DELIVERY) {
    const delivery_id = cursor.string("delivery_id");
    const attempt = cursor.u32("attempt");
    const envelope = cursor.envelope();
    cursor.expectEnd();
    return { kind: "delivery", delivery_id, envelope, attempt };
  }
  if (kind === SERVER_DELIVERY_OUTCOME) {
    const delivery_id = cursor.string("delivery_id");
    const outcome = cursor.outcome();
    const reason = cursor.optionalNackReason();
    cursor.expectEnd();
    return {
      kind: "delivery_outcome",
      outcome: deliveryOutcome(delivery_id, outcome, reason),
    };
  }
  throw new SdkError("E_PROTOCOL", `unknown zornmesh frame type ${kind}`, false);
}

function frameStatus(status: number): SendStatus {
  if (status === 1) {
    return "accepted";
  }
  if (status === 2) {
    return "rejected";
  }
  if (status === 3) {
    return "validation_failed";
  }
  throw new SdkError("E_PROTOCOL", `unknown zornmesh result status ${status}`, false);
}

function deliveryOutcome(
  delivery_id: string,
  outcome: CoordinationOutcome,
  reason: NackReasonCategory | null,
): DeliveryOutcome {
  return {
    delivery_id,
    kind: outcome.kind,
    stage: outcome.stage,
    code: outcome.code,
    message: outcome.message,
    retryable: outcome.retryable,
    terminal: outcome.terminal,
    reason,
    outcome,
  };
}

function frameBody(kind: number): number[] {
  return [MAGIC[0], MAGIC[1], 0x00, ENVELOPE_WIRE_VERSION, kind];
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

function takeFrame(buffer: Uint8Array): { body: Uint8Array; remaining: Uint8Array } | null {
  if (buffer.byteLength < 4) {
    return null;
  }
  const length = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength).getUint32(0);
  if (length > MAX_FRAME_BYTES) {
    throw new SdkError(
      "E_PAYLOAD_LIMIT",
      `zornmesh frame is ${length} bytes; maximum is ${MAX_FRAME_BYTES} bytes`,
      false,
    );
  }
  if (buffer.byteLength < 4 + length) {
    return null;
  }
  return {
    body: buffer.slice(4, 4 + length),
    remaining: buffer.slice(4 + length),
  };
}

class Cursor {
  private offset = 0;

  constructor(private readonly bytes: Uint8Array) {}

  expectMagic(): void {
    const magic = this.takeExact(MAGIC.length, "magic");
    if (magic[0] !== MAGIC[0] || magic[1] !== MAGIC[1]) {
      throw new SdkError("E_PROTOCOL", "invalid zornmesh frame magic", false);
    }
  }

  expectVersion(): void {
    const version = this.u16("version");
    if (version !== ENVELOPE_WIRE_VERSION) {
      throw new SdkError("E_PROTOCOL", `unsupported zornmesh frame version ${version}`, false);
    }
  }

  expectEnd(): void {
    if (this.offset !== this.bytes.byteLength) {
      throw new SdkError("E_PROTOCOL", "zornmesh frame contains trailing bytes", false);
    }
  }

  envelope(): Envelope {
    const source_agent = this.string("source_agent");
    const subject = this.string("subject");
    const timestamp_unix_ms = this.u64("timestamp_unix_ms");
    const correlation_id = this.string("correlation_id");
    const content_type = this.string("payload_content_type");
    const payload = this.bytesField("payload");
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

  u8(field: string): number {
    return this.takeExact(1, field)[0];
  }

  u16(field: string): number {
    const bytes = this.takeExact(2, field);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint16(0);
  }

  u32(field: string): number {
    const bytes = this.takeExact(4, field);
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0);
  }

  u64(field: string): number {
    const bytes = this.takeExact(8, field);
    return Number(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0));
  }

  string(field: string): string {
    return new TextDecoder().decode(this.bytesField(field));
  }

  bytesField(field: string): Uint8Array {
    return this.takeExact(this.u32(field), field);
  }

  bool(field: string): boolean {
    const value = this.u8(field);
    if (value === 0) {
      return false;
    }
    if (value === 1) {
      return true;
    }
    throw new SdkError("E_PROTOCOL", `zornmesh frame field ${field} has invalid boolean ${value}`, false);
  }

  outcome(): CoordinationOutcome {
    const kind = this.outcomeKind("outcome_kind");
    const stage = this.outcomeStage("outcome_stage");
    return {
      version: COORDINATION_CONTRACT_VERSION,
      kind,
      stage,
      code: this.string("outcome_code"),
      message: this.string("outcome_message"),
      retryable: this.bool("outcome_retryable"),
      terminal: this.bool("outcome_terminal"),
      delivery_attempts: this.u32("outcome_delivery_attempts"),
    };
  }

  optionalOutcome(): CoordinationOutcome | null {
    return this.bool("has_durable_outcome") ? this.outcome() : null;
  }

  optionalNackReason(): NackReasonCategory | null {
    if (!this.bool("has_nack_reason")) {
      return null;
    }
    const reason = this.string("nack_reason");
    if (!isNackReason(reason)) {
      throw new SdkError("E_PROTOCOL", `unknown zornmesh NACK reason ${reason}`, false);
    }
    return reason;
  }

  private outcomeKind(field: string): CoordinationOutcomeKind {
    const kind = this.string(field);
    if (!isCoordinationOutcomeKind(kind)) {
      throw new SdkError("E_PROTOCOL", `unknown zornmesh outcome kind ${kind}`, false);
    }
    return kind;
  }

  private outcomeStage(field: string): CoordinationStage {
    const stage = this.string(field);
    if (!isCoordinationStage(stage)) {
      throw new SdkError("E_PROTOCOL", `unknown zornmesh outcome stage ${stage}`, false);
    }
    return stage;
  }

  private takeExact(length: number, field: string): Uint8Array {
    const end = this.offset + length;
    if (end > this.bytes.byteLength) {
      throw new SdkError("E_PROTOCOL", `truncated zornmesh frame field ${field}`, false);
    }
    const value = this.bytes.slice(this.offset, end);
    this.offset = end;
    return value;
  }
}

function appendBytes(left: Uint8Array, right: Uint8Array): Uint8Array {
  if (left.byteLength === 0) {
    return new Uint8Array(right);
  }
  const output = new Uint8Array(left.byteLength + right.byteLength);
  output.set(left, 0);
  output.set(right, left.byteLength);
  return output;
}

function payloadBytes(payload: Uint8Array | ArrayBuffer | string): Uint8Array {
  if (typeof payload === "string") {
    return new TextEncoder().encode(payload);
  }
  if (payload instanceof Uint8Array) {
    return payload;
  }
  return new Uint8Array(payload);
}

function sdkErrorFromSendResult(result: SendResult): SdkError {
  return new SdkError(codeFromString(result.code), result.message);
}

function sdkErrorFromDeliveryOutcome(result: DeliveryOutcome): SdkError {
  return new SdkError(codeFromString(result.code), result.message, result.retryable);
}

function sendResultFromError(error: SdkError): SendResult {
  if (error.code === "E_DAEMON_UNREACHABLE") {
    return {
      status: "daemon_unreachable",
      code: error.code,
      message: error.message,
      outcome: coordinationOutcome(
        "retryable",
        "transport",
        error.code,
        error.message,
        true,
        false,
        0,
      ),
      durable_outcome: null,
    };
  }
  if (["E_SUBJECT_VALIDATION", "E_PAYLOAD_LIMIT", "E_PROTOCOL"].includes(error.code)) {
    return {
      status: "validation_failed",
      code: error.code,
      message: error.message,
      outcome: coordinationOutcome(
        "failed",
        "protocol",
        error.code,
        error.message,
        false,
        true,
        0,
      ),
      durable_outcome: null,
    };
  }
  return {
    status: "rejected",
    code: error.code,
    message: error.message,
    outcome: coordinationOutcome(
      "rejected",
      "transport",
      error.code,
      error.message,
      false,
      true,
      0,
    ),
    durable_outcome: null,
  };
}

function coordinationOutcome(
  kind: CoordinationOutcomeKind,
  stage: CoordinationStage,
  code: string,
  message: string,
  retryable: boolean,
  terminal: boolean,
  delivery_attempts: number,
): CoordinationOutcome {
  return {
    version: COORDINATION_CONTRACT_VERSION,
    kind,
    stage,
    code,
    message,
    retryable,
    terminal,
    delivery_attempts,
  };
}

function codeFromString(code: string): SdkErrorCode {
  return SDK_ERROR_CODES.includes(code as SdkErrorCode) ? (code as SdkErrorCode) : "E_DAEMON_IO";
}

function asSdkError(error: unknown): SdkError {
  if (error instanceof SdkError) {
    return error;
  }
  return new SdkError("E_DAEMON_IO", errorMessage(error));
}

function daemonUnreachableAutospawnDisabled(): SdkError {
  return new SdkError(
    "E_DAEMON_UNREACHABLE",
    "daemon is unreachable and ZORN_NO_AUTOSPAWN=1 is set; run `zornmesh daemon` and retry",
  );
}

function daemonUnreachableFromConnect(error: unknown): SdkError {
  return new SdkError(
    "E_DAEMON_UNREACHABLE",
    `daemon is not accepting connections; run \`zornmesh daemon\` and retry: ${errorMessage(error)}`,
  );
}

function readinessTimeout(timeoutMs: number, lastError: SdkError | undefined): SdkError {
  const detail = lastError ? ` last daemon state: ${lastError.code}` : " last daemon state: unreachable";
  return new SdkError(
    "E_DAEMON_READINESS_TIMEOUT",
    `daemon did not become ready within ${timeoutMs} ms; retry or run \`zornmesh daemon\` explicitly;${detail}`,
  );
}

function isRetryableCode(code: SdkErrorCode): boolean {
  return code === "E_DAEMON_UNREACHABLE" || code === "E_DAEMON_READINESS_TIMEOUT";
}

function errorCategoryForCode(code: SdkErrorCode): ErrorCategory {
  if (
    code === "E_LOCAL_TRUST_UNSAFE" ||
    code === "E_INVALID_CONFIG" ||
    code === "E_SUBJECT_VALIDATION" ||
    code === "E_DELIVERY_VALIDATION"
  ) {
    return "validation";
  }
  if (code === "E_ELEVATED_PRIVILEGE") {
    return "authorization";
  }
  if (code === "E_DAEMON_UNREACHABLE") {
    return "reachability";
  }
  if (code === "E_DAEMON_READINESS_TIMEOUT") {
    return "timeout";
  }
  if (code === "E_PAYLOAD_LIMIT") {
    return "payload_limit";
  }
  if (code === "E_PROTOCOL") {
    return "protocol";
  }
  if (code === "E_PERSISTENCE_UNAVAILABLE") {
    return "persistence_unavailable";
  }
  return "internal";
}

function isCoordinationOutcomeKind(value: string): value is CoordinationOutcomeKind {
  return COORDINATION_OUTCOME_KINDS.includes(value as CoordinationOutcomeKind);
}

function isCoordinationStage(value: string): value is CoordinationStage {
  return COORDINATION_STAGES.includes(value as CoordinationStage);
}

function isNackReason(value: string): value is NackReasonCategory {
  return NACK_REASON_CATEGORIES.includes(value as NackReasonCategory);
}

function autoSpawnFromEnv(value: string | undefined): boolean {
  const normalized = value?.trim();
  return !(normalized === "1" || normalized === "true" || normalized === "TRUE" || normalized === "yes" || normalized === "YES");
}

function resolveSocketPathFromEnv(): string {
  const configured = process.env[ENV_SOCKET_PATH];
  if (configured !== undefined) {
    return configured;
  }
  const runtimeDir = process.env.XDG_RUNTIME_DIR;
  if (runtimeDir) {
    return `${runtimeDir}/zorn-mesh/zorn.sock`;
  }
  const uid = effectiveUid();
  return `/run/user/${uid ?? "unknown"}/zorn-mesh/zorn.sock`;
}

function effectiveUid(): number | undefined {
  return typeof process.getuid === "function" ? process.getuid() : undefined;
}

function positiveMillis(value: number, field: string): number {
  if (!Number.isFinite(value) || value <= 0) {
    throw new SdkError("E_INVALID_CONFIG", `${field} must be a positive millisecond value`, false);
  }
  return value;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)));
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
