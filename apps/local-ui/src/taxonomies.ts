// Shared state taxonomies for CLI / SDK / daemon / UI.
//
// These taxonomies are the contract every Epic 6 surface must obey. Unknown
// or future states render the explicit `unknown` fallback rather than collapsing
// into a silent success state.

export const agentStatus = [
  "active",
  "stale",
  "errored",
  "disconnected",
  "reconnecting",
  "unknown",
] as const;
export type AgentStatus = (typeof agentStatus)[number];

export const deliveryState = [
  "pending",
  "queued",
  "accepted",
  "delivered",
  "acknowledged",
  "rejected",
  "failed",
  "cancelled",
  "replayed",
  "dead_lettered",
  "stale",
  "unknown",
] as const;
export type DeliveryState = (typeof deliveryState)[number];

export const traceCompleteness = [
  "complete",
  "partial",
  "not_found",
  "unavailable",
  "unknown",
] as const;
export type TraceCompleteness = (typeof traceCompleteness)[number];

export const daemonHealth = [
  "starting",
  "ready",
  "degraded",
  "reconnecting",
  "unavailable",
  "schema_mismatch",
  "session_expired",
  "unknown",
] as const;
export type DaemonHealth = (typeof daemonHealth)[number];

export const trustPosture = [
  "loopback_only",
  "session_protected",
  "schema_pinned",
  "stale",
  "unsafe",
  "unknown",
] as const;
export type TrustPosture = (typeof trustPosture)[number];

export const taxonomies = {
  agentStatus,
  deliveryState,
  traceCompleteness,
  daemonHealth,
  trustPosture,
} as const;

export function fallbackLabel<T extends string>(
  value: T | undefined,
  allowed: readonly T[],
): T | "unknown" {
  if (value === undefined) return "unknown";
  return (allowed as readonly string[]).includes(value) ? value : ("unknown" as const);
}
