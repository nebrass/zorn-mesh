# Ralph Fix Plan

## Stories to Implement

### First Local Mesh and SDK Bootstrap
> Goal: A developer can install/run `zornmesh`, start or auto-spawn one trustworthy local daemon, use stable CLI output, and send/receive a first basic envelope through Rust/TypeScript SDK surfaces.

- [x] Story 1.1: Create Buildable Workspace and Command Skeleton
- [x] Story 1.2: Establish Local Daemon Rendezvous and Trust Checks
- [x] Story 1.3: Connect Rust SDK to Auto-Spawned Daemon
- [x] Story 1.4: Send First Local Publish/Subscribe Envelope
- [x] Story 1.5: Add TypeScript SDK Bootstrap Parity
- [x] Story 1.6: Stabilize CLI Read Outputs and Exit Contracts
- [x] Story 1.7: Provide Doctor, Shutdown, and Shell Completion Basics
### Reliable Agent Coordination
> Goal: Agents can coordinate beyond the first message using request/reply, pull leases, streaming, ACK/NACK, cancellation, idempotency, durable subscriptions, backpressure, and per-call context. **Durability contract:** Stories 2.1-2.8 may claim durable ACK, lease, idempotency, subscription, retry, or backpressure state only after the relevant SQLite/sqlx commit succeeds. In-memory-only state must return typed persistence-unavailable or unsupported outcomes and must never claim durable success.

- [x] Story 2.1: Establish Coordination Result and ACK/NACK Contract
- [x] Story 2.2: Send Correlated Request/Reply with Timeout
- [x] Story 2.3: Fetch, Lease, ACK, and NACK Pulled Envelopes
- [x] Story 2.4: Add Idempotency Keys and Retry-Safe Sends
- [x] Story 2.5: Stream Response Chunks with Byte-Budget Flow Control
- [x] Story 2.6: Cancel In-Flight Requests and Streams
- [x] Story 2.7: Resume Durable Subscriptions After Daemon Restart
- [x] Story 2.8: Surface Backpressure at Queue and Lease Bounds
### Agent Identity, Capabilities, and Host Bridges
> Goal: Developers can see who is on the mesh, what each agent can do, safely gate high-privilege capabilities, and bridge existing MCP hosts without modifying those hosts.

- [x] Story 3.1: Register Minimal AgentCard Identity
- [x] Story 3.2: Advertise and Resolve Symmetric Capabilities
  > As an adopter
  > I want agents to advertise both offered and consumed capabilities
  > So that I can understand what each agent can do and how agents may interact.
  > AC: Given an agent registers identity metadata, When it submits offered and consumed capability descriptors, Then the daemon validates capability IDs, versions, schema references, and declared direction, And invalid or unsupported capability descriptors are rejected with stable typed errors.
  > AC: Given capabilities are accepted, When an adopter lists agents or inspects one agent, Then the output includes the agent identity, offered capabilities, consumed capabilities, versions, and safe summaries, And the output is available through SDK and CLI structured formats.
  > AC: Given capability schemas are provided for capability input/output contracts, When the daemon stores or resolves the capability, Then schema metadata is associated with the capability without generating the canonical internal envelope model from it, And TypeBox/JSON Schema ownership remains distinct from Protobuf internal model ownership.
  > AC: Given an agent updates its capability declaration during a session, When the update is valid, Then the registry updates deterministically and emits an observable capability-change event, And invalid updates do not partially mutate the registry.
  > AC: Given capability schema support is introduced, When capability fixtures are created, Then the supported schema dialect, allowed annotation subset, secret-field annotation form, versioning rules, and unsupported-schema errors are pinned in the central capability fixture manifest, And schema fixtures run in the daemon, Rust SDK, TypeScript SDK, CLI, and bridge CI matrix.
  > AC: Given capability resolution conformance tests run, When offered-only, consumed-only, both-directions, invalid-schema-reference, and update scenarios execute, Then resolved capability output is deterministic across Rust SDK, TypeScript SDK, CLI, and fixture expectations.
  > Spec: specs/planning-artifacts/epics.md#story-3-2
- [ ] Story 3.3: Gate High-Privilege Capabilities by Local Allowlist
  > As an operator
  > I want high-privilege capabilities to be denied unless explicitly allow-listed
  > So that one local agent cannot silently advertise or invoke dangerous actions.
  > AC: Given a capability is marked high-privilege by local policy, When an agent attempts to advertise it without an allowlist entry, Then registration rejects that capability with a stable authorization error, And the agent may still register non-rejected capabilities when safe to do so.
  > AC: Given an agent is not allow-listed for a high-privilege capability, When it attempts to invoke or consume that capability, Then the daemon rejects the invocation before dispatch, And no downstream agent receives the unauthorized request.
  > AC: Given an operator provides a valid local allowlist entry, When the allow-listed agent advertises or invokes the high-privilege capability, Then the operation is permitted according to the local policy, And the decision is recorded as an observable authorization event.
  > AC: Given the allowlist file or config is missing, malformed, or unreadable, When high-privilege policy evaluation runs, Then default policy is deny, And diagnostics explain the safe remediation path without exposing secrets.
  > AC: Given an allowlist entry is revoked while an agent session is active, When policy reload or policy re-evaluation observes the revocation, Then the revoked capability is removed or marked unavailable for that agent before any new dispatch, And queued or undispatched invocations are denied with a stable authorization error while already-dispatched work follows the documented in-flight policy and emits an audit event.
  > AC: Given authorization conformance tests run, When deny-by-default, allow-listed advertise, allow-listed invoke, malformed-policy, active-session-revocation, and unauthorized-dispatch scenarios execute, Then no high-privilege capability bypasses policy and every denial has a stable error code.
  > Spec: specs/planning-artifacts/epics.md#story-3-3
- [ ] Story 3.4: Enforce Local Socket Permission Model on Agent Connections
  > As an operator
  > I want the mesh to reject agent connections that do not satisfy the local socket trust model
  > So that cross-user or unsafe local processes cannot join the bus by accident.
  > AC: Given an agent connects over the local IPC transport, When the daemon evaluates peer credentials and socket ownership, Then the connection is accepted only when it matches the invoking user/session trust boundary, And accepted connection metadata is associated with the registered agent identity.
  > AC: Given socket file mode, ownership, or peer credentials do not satisfy the trust model, When an agent attempts to connect or register, Then the daemon rejects the connection or registration with a stable permission error, And no agent identity or capability state is created from that attempt.
  > AC: Given an unsupported or unsafe socket form is used, When the daemon detects the unsafe form during startup or connection handling, Then it refuses the unsafe path with remediation text, And the refusal is visible through CLI/doctor diagnostics.
  > AC: Given a valid connection later loses its trusted transport state or disconnects unexpectedly, When the daemon observes the disconnect, Then agent presence is updated deterministically, And future routing does not treat the disconnected agent as connected.
  > AC: Given local trust conformance tests run, When valid user, wrong user, unsafe permission, unsupported socket form, and disconnect scenarios execute, Then accepted/rejected outcomes are deterministic and no unauthorized agent receives mesh traffic.
  > Spec: specs/planning-artifacts/epics.md#story-3-4
- [ ] Story 3.5: Redact Secrets Across Identity, Capability, and Delivery Surfaces
  > As an adopter
  > I want fields marked secret to stay redacted across all observable surfaces
  > So that agent coordination does not leak credentials or sensitive payload fragments.
  > AC: Given an SDK caller marks a field or value as secret using the supported language mechanism, When the field flows through identity metadata, capability payloads, delivery results, errors, logs, traces, audit records, dead letters, inspect output, or CLI output, Then the raw secret value never appears, And a stable redaction marker is used where display is necessary.
  > AC: Given a secret appears inside nested payload data or structured safe details, When serialization, validation, logging, or error mapping occurs, Then redaction is applied before the value reaches any external or persisted diagnostic surface, And redaction does not break schema validation of the non-secret surrounding structure.
  > AC: Given a capability schema declares secret fields, When the daemon validates and stores capability metadata, Then the schema's secret annotations are preserved for downstream redaction decisions, And safe summaries omit or redact those fields.
  > AC: Given an unsupported or ambiguous secret marker is used, When the SDK or daemon encounters it, Then the system returns a stable validation error or treats the value as secret according to the documented safe default, And the behavior is fixture-covered.
  > AC: Given the shared redaction contract is introduced, When identity, capability, delivery, trace, audit, inspect, error, and CLI fixtures are created, Then supported secret markers, stable replacement text, nested redaction behavior, and ambiguous-marker defaults are pinned centrally, And every surface test consumes the same redaction fixture values and fails on raw secret emission.
  > AC: Given redaction conformance tests run, When Rust SDK, TypeScript SDK, daemon logs, audit records, CLI inspect, metrics/traces, and dead-letter scenarios execute, Then no raw fixture secret appears in captured outputs, And tests fail on any unredacted emission.
  > Spec: specs/planning-artifacts/epics.md#story-3-5
- [ ] Story 3.6: Canonicalize Agent Identity Across Multiple Host Connections
  > As an adopter
  > I want repeated connections from the same logical agent or host to resolve to one canonical mesh identity
  > So that routing and trace history do not fragment across duplicate connection records.
  > AC: Given the same logical agent connects through multiple supported connection paths or sessions, When the identity metadata matches the canonicalization rules, Then the daemon resolves those connections to one canonical agent identity, And presence output shows connection/source details without creating duplicate logical agents.
  > AC: Given two connections claim the same identity but have incompatible metadata, When canonicalization evaluates them, Then the daemon rejects or quarantines the conflicting connection with a stable conflict error, And the existing canonical identity is not overwritten.
  > AC: Given raw host identity metadata differs from the canonical mesh identity shape, When normalization succeeds, Then both raw and normalized identity forms remain available for audit/debugging, And routing, capability lookup, and trace output use the canonical identity.
  > AC: Given a canonical agent has multiple active connections, When one connection disconnects, Then the agent remains present if another valid connection is active, And routing uses the remaining valid connection according to deterministic selection rules.
  > AC: Given canonical identity tests run, When same-agent reconnect, multiple host connections, incompatible duplicate, raw/normalized storage, and partial disconnect scenarios execute, Then agent presence and routing behavior remain deterministic.
  > Spec: specs/planning-artifacts/epics.md#story-3-6
- [ ] Story 3.7: Connect MCP Hosts Through `zornmesh stdio --as-agent`
  > As a developer using an existing MCP host
  > I want to connect that host to zorn-mesh through `zornmesh stdio --as-agent`
  > So that existing tools can join the local mesh without host modification.
  > AC: Given an MCP-compatible host launches `zornmesh stdio --as-agent <id>`, When the host performs MCP initialize using the supported protocol version, Then the bridge completes initialization and registers the host as a mesh agent, And the registered identity uses the AgentCard, capability-resolution, allowlist, socket-permission, and secret-redaction contracts established by Stories 3.1-3.5.
  > AC: Given the MCP host sends requests before successful initialize, repeats initialize, or sends messages out of the supported bridge sequence, When the stdio bridge validates MCP sequencing, Then it returns stable protocol/sequence errors without registering mesh identity or capabilities prematurely, And no mesh operation is dispatched until initialize and identity registration both complete.
  > AC: Given the MCP host initializes with an unsupported protocol version, When the bridge validates the initialize request, Then it returns a stable unsupported-protocol-version error using safe MCP-compatible error details, And no agent identity, capability, or presence state is created.
  > AC: Given the bridge receives MCP requests or tool calls supported by the mesh bridge, When it maps them into internal mesh operations, Then request identity, correlation ID, trace context, and capability metadata are preserved where representable, And unsupported mappings are not silently dropped.
  > AC: Given the daemon is unavailable when the stdio bridge starts, When bridge initialization attempts mesh connection, Then the bridge follows the same daemon connect/auto-spawn policy as other CLI/SDK surfaces, And failures are reported to the host with stable, safe error information.
  > AC: Given the host process exits or stdio closes, When the bridge detects closure, Then the corresponding mesh connection and presence state are cleaned up deterministically, And no orphaned agent remains visible as connected.
  > AC: Given MCP bridge conformance tests run, When initialize success, out-of-sequence MCP input, duplicate initialize, unsupported protocol version, daemon unavailable, supported request mapping, policy-denied capability, redacted secret field, host exit, and malformed MCP input scenarios execute, Then bridge behavior is deterministic and pinned to the supported MCP version fixture set.
  > Spec: specs/planning-artifacts/epics.md#story-3-7
- [ ] Story 3.8: Degrade Gracefully for Baseline MCP Capability Limits
  > As a developer bridging an MCP host
  > I want unsupported mesh capabilities to return explicit unsupported-capability results
  > So that baseline MCP hosts fail clearly instead of appearing broken or silently losing behavior.
  > AC: Given a connected MCP host supports only baseline MCP capability shapes, When the bridge exposes mesh capabilities to that host, Then only capabilities representable on the MCP wire are exposed, And non-representable capabilities are withheld or marked unsupported according to documented rules.
  > AC: Given the host invokes a mesh capability that cannot be represented on baseline MCP, When the bridge evaluates the invocation, Then it returns a named unsupported-capability result, And the result includes safe remediation text or equivalent CLI/SDK handoff where available.
  > AC: Given a mesh operation partially maps to MCP but loses required semantics such as streaming, delivery ACK, trace context, or high-privilege policy, When the bridge evaluates the mapping, Then it refuses or degrades explicitly according to fixture-backed rules, And it does not pretend full mesh semantics were preserved.
  > AC: Given unsupported-capability results occur, When the daemon and CLI surfaces observe them, Then they are visible as structured events and stable errors, And secret payload data remains redacted.
  > AC: Given MCP graceful-degradation tests run, When supported capability, unsupported capability, partial mapping, policy-denied, and trace-context-limited scenarios execute, Then the bridge produces deterministic results pinned to the MCP version fixture set.
  > Spec: specs/planning-artifacts/epics.md#story-3-8
### Forensic Persistence, Trace, and Recovery
> Goal: Developers can reconstruct, inspect, tail, replay, and recover multi-agent conversations from durable local evidence when something breaks.

- [ ] Story 4.1: Persist Envelopes, Audit Entries, and Trace Indexes
  > As a developer
  > I want every accepted envelope to become durable local evidence
  > So that I can later inspect, trace, replay, and audit what agents actually did.
  > AC: Given the daemon accepts an envelope for durable processing, When the persistence writer commits it, Then the envelope record includes daemon sequence, message ID, source agent, target or subject, timestamp, correlation ID, trace ID, parent/lineage metadata, delivery state, and safe payload metadata, And durable ACK is emitted only after the relevant SQLite/sqlx transaction commits; temporary memory, queue buffering, WAL intent, or process-local cache state never counts as durable success.
  > AC: Given the accepted envelope changes delivery or authorization state, When the state transition occurs, Then an audit entry is written with actor/agent identity, action, capability or subject, correlation ID, trace ID, prior-message lineage where available, and safe outcome details, And secret fields are redacted before persistence.
  > AC: Given an envelope, delivery-state change, or authorization decision is persisted, When its audit entry is written, Then the audit row links to the relevant envelope/message ID, daemon sequence, previous audit hash, current audit hash, actor, action, state transition, and safe outcome details, And the envelope record, trace indexes, audit entry, and daemon sequence assignment are committed atomically or not visible as durable.
  > AC: Given trace and correlation lookup are required by downstream trace/UI stories, When messages and audit entries are persisted, Then queryable indexes exist for correlation ID, trace ID, agent ID, subject, delivery state, and time window, And index naming follows the architecture conventions.
  > AC: Given persistence is unavailable, migration state is invalid, or disk-full behavior is encountered, When the daemon tries to persist accepted work, Then the operation fails with stable typed persistence errors or enters the documented read-degraded posture, And no durable ACK is emitted for uncommitted work.
  > AC: Given the daemon opens a corrupt, partially migrated, future-schema, or unreadable store, When startup or recovery validation runs, Then the daemon refuses unsafe writes or enters the documented read-degraded posture with stable typed diagnostics, And no durable ACK is emitted until store integrity and migration state are safe.
  > AC: Given two daemon starts or migration workers race while schema migration is required, When migration locking runs, Then exactly one migrator applies forward-only migrations atomically, And failures leave pre-migration state intact while losing processes refuse startup with stable diagnostics.
  > AC: Given the daemon crashes before, during, or after a persistence transaction, When it restarts, Then fully committed records, daemon sequences, audit hashes, and trace indexes are recovered exactly once, And partially committed or ambiguous work is not reported as durable and is surfaced through stable recovery diagnostics.
  > AC: Given SQLite WAL recovery benchmarks run against the reference 7-day default-retention audit database, When the daemon performs startup recovery, Then recovery completes in <= 2 seconds on the v0.1 reference platform, And benchmark failures block release readiness.
  > AC: Given persistence conformance tests run, When accepted envelope, commit failure, audit-hash-linkage, atomic-sequence-assignment, corrupt-store-open, redaction, indexed query, daemon restart, crash-before-after-commit, and daemon crash scenarios execute, Then accepted records are recoverable after restart and failed records are not reported as durable.
  > Spec: specs/planning-artifacts/epics.md#story-4-1
- [ ] Story 4.2: Propagate Tracecontext and Emit OpenTelemetry Schema
  > As a developer
  > I want every mesh operation to carry trace context and emit documented telemetry
  > So that I can follow causality across agents without instrumenting each hop by hand.
  > AC: Given an envelope enters the mesh with W3C `traceparent` and `tracestate` values, When the daemon routes, persists, delivers, retries, or dead-letters the envelope, Then trace context is propagated to downstream operations without adopter intervention, And missing trace context is generated according to documented rules.
  > AC: Given an envelope enters with malformed `traceparent` or `tracestate`, When trace context is validated, Then malformed context is rejected or normalized according to one documented rule before routing, And malformed values are never propagated downstream or emitted as valid telemetry.
  > AC: Given request/reply, streaming, publish/subscribe, fetch/lease, ACK/NACK, and cancellation operations occur, When telemetry is enabled for local observation or test capture, Then spans and metrics follow the documented `zornmesh.*` schema, And high-cardinality values such as correlation IDs and subjects are not emitted as metric labels.
  > AC: Given trace data is recorded for a mesh operation, When the operation crosses agents or delivery states, Then parent/child span relationships preserve causality across the full path, And late, retry, replay, dead-letter, and cancellation states are represented as explicit events or attributes.
  > AC: Given telemetry export is not configured, When normal daemon operations run, Then no outbound telemetry network connection is made, And local trace/audit evidence remains available for CLI and future UI surfaces.
  > AC: Given an OpenTelemetry exporter is configured but unreachable, slow, or returning errors, When mesh operations emit telemetry, Then broker delivery, persistence, and ACK paths are not blocked beyond the documented budget, And exporter failures are bounded, observable through health/diagnostic events, and do not drop local audit or trace evidence.
  > AC: Given metrics include labels derived from agents, subjects, capability IDs, error categories, or delivery states, When label values exceed the documented cardinality cap, Then excess values are bucketed or suppressed according to the telemetry schema, And correlation IDs, trace IDs, message IDs, raw subjects, and payload fragments never become metric labels.
  > AC: Given observability conformance tests run, When trace propagation, schema validation, malformed traceparent, malformed tracestate, no-export-default, exporter unreachable, exporter slow, high-cardinality, cardinality cap, and multi-hop causality scenarios execute, Then output matches the documented telemetry schema and fixture expectations.
  > Spec: specs/planning-artifacts/epics.md#story-4-2
- [ ] Story 4.3: Capture Dead Letters with Structured Failure Reasons
  > As a developer
  > I want undeliverable or exhausted envelopes to land in a dead-letter queue with clear causes
  > So that failures remain inspectable and recoverable instead of disappearing.
  > AC: Given an envelope cannot be delivered because no eligible recipient exists, a TTL expires, retry budget is exhausted, validation fails after acceptance, or delivery repeatedly fails, When terminal failure is reached, Then the broker writes a dead-letter record with message ID, source, intended target/subject, correlation ID, trace ID, terminal state, failure category, and safe details, And the dead-letter record is persisted through the SQLite/sqlx durable store before the original envelope is considered terminal.
  > AC: Given a dead-letter record includes payload metadata, When it is persisted or displayed, Then secret fields are redacted according to the shared redaction contract, And the record preserves enough metadata for trace, inspect, and future UI recovery flows.
  > AC: Given multiple delivery attempts occurred before dead-lettering, When the DLQ record is created, Then attempt count, last failure category, and relevant timing metadata are captured, And retry history can be correlated to audit/trace entries.
  > AC: Given a developer queries dead letters by subject, agent, correlation ID, failure category, or time window, When matching records exist, Then the CLI/API returns structured results with stable schema and clear empty-state behavior.
  > AC: Given DLQ conformance tests run, When no-recipient, TTL-expired, retry-exhausted, validation-terminal, redaction, corrupt-store, restart-recovery, and filtered-query scenarios execute, Then each terminal failure creates exactly one inspectable dead-letter record.
  > Spec: specs/planning-artifacts/epics.md#story-4-3
- [ ] Story 4.4: Inspect Persistence State with Structured Filters
  > As a developer
  > I want to inspect persisted messages, dead letters, audit entries, and runtime metadata with filters
  > So that I can answer "what happened?" without opening SQLite by hand.
  > AC: Given persisted messages, dead letters, audit entries, schema metadata, and release-integrity metadata exist or are unavailable, When the developer runs the inspect command or SDK/API equivalent, Then the response clearly distinguishes available data, unavailable data, empty states, and unsupported placeholders, And output is available in human and JSON modes.
  > AC: Given the developer filters by correlation ID, trace ID, agent ID, subject, delivery state, failure category, or time window, When matching records exist, Then only matching records are returned in deterministic order, And filter chips/metadata in structured output explain which filters were applied.
  > AC: Given no matching records exist, When an inspect query returns empty, Then the output explains the empty state and suggests relevant next actions such as trace, tail, doctor, or retention checks, And JSON mode returns an explicit empty data collection, not omitted fields.
  > AC: Given persisted records contain redacted payloads or secret markers, When inspect output is rendered, Then raw secret values are never emitted, And redaction markers remain understandable in both human and JSON modes.
  > AC: Given an inspect query could return more records than the documented default or maximum page size, When the CLI/API renders results, Then output is paginated with deterministic ordering, explicit limit metadata, and a stable next-cursor or completion marker, And over-limit requests return a stable validation error or are clamped according to documented rules.
  > AC: Given inspect conformance tests run, When filtered message, DLQ, audit, empty, redacted, huge result set, pagination cursor, over-limit request, and unavailable-metadata scenarios execute, Then output shapes, ordering, stdout/stderr separation, and exit codes match fixtures.
  > Spec: specs/planning-artifacts/epics.md#story-4-4
- [ ] Story 4.5: Reconstruct Conversation Timeline by Correlation ID
  > As a developer debugging a broken multi-agent workflow
  > I want `zornmesh trace <correlation_id>` to rebuild the ordered conversation timeline
  > So that I can understand every participating agent, message, and delivery state without stitching logs by hand.
  > AC: Given persisted messages and audit entries share a correlation ID, When the developer runs `zornmesh trace <correlation_id>`, Then the command returns an ordered timeline containing every available envelope, hop, participating agent, delivery state, timestamp, and safe payload summary, And ordering is based on daemon sequence/persisted chronology, not browser or client receipt time.
  > AC: Given the trace includes retries, late arrivals, replays, dead letters, or cancellations, When the timeline is rendered, Then each exceptional state is explicitly marked in human and JSON output, And the timeline does not collapse partial failure into success.
  > AC: Given no records exist for the requested correlation ID, When the trace command runs, Then it returns a stable not-found result with remediation hints, And JSON output preserves the stable top-level schema with empty data and warnings.
  > AC: Given records are missing because of retention, corruption, or partial message loss, When the trace command detects a gap, Then the output marks the trace as partial/gap detected, And points to inspect, doctor, retention, or audit verification next steps.
  > AC: Given trace reconstruction tests run, When complete, missing, partial, retry, replay, dead-letter, and cancellation timelines execute, Then timeline output is deterministic and fixture-covered for both human and JSON modes.
  > Spec: specs/planning-artifacts/epics.md#story-4-5
- [ ] Story 4.6: Reconstruct Span Trees for Request/Reply and Streaming
  > As a developer debugging causality
  > I want trace output to show parent/child span relationships for request/reply and streaming exchanges
  > So that I can see which agent action caused each downstream message or stream chunk.
  > AC: Given a request/reply exchange persists trace IDs, span IDs, parent IDs, correlation IDs, and agent references, When the developer requests span-tree reconstruction, Then the output shows parent/child relationships from initial request through reply, And missing or invalid parent references are explicitly marked.
  > AC: Given a streaming exchange emits multiple chunks, When the span tree is reconstructed, Then stream chunks are grouped under the correct stream/request context in sequence order, And final, cancelled, failed, or gap states are represented explicitly.
  > AC: Given a trace includes fan-out, retry, replay, or dead-letter branches, When the span tree is rendered, Then branches are labeled by relationship type such as caused-by, responds-to, replayed-from, retry-of, or dead-letter-terminal, And relationship labels are stable for future UI accessibility semantics.
  > AC: Given persisted span relationships contain a self-parent, duplicate edge, or cycle, When span-tree reconstruction runs, Then the cycle is detected, traversal terminates deterministically, and the affected nodes are marked invalid/partial, And output does not recurse forever, drop unrelated valid branches, or invent corrected causality.
  > AC: Given partial trace data is available, When parent/child reconstruction cannot be completed, Then the output reports partial reconstruction with safe diagnostics, And it does not invent or infer missing causality edges as facts.
  > AC: Given span-tree tests run, When request/reply, streaming, fan-out, retry, replay, self-parent, cycle, duplicate edge, missing-parent, and partial-data scenarios execute, Then reconstructed causality is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-4-6
- [ ] Story 4.7: Live-Tail Envelopes by Subject Pattern
  > As a developer
  > I want to live-tail envelope flow by subject pattern
  > So that I can watch the mesh in real time while agents coordinate.
  > AC: Given a daemon is receiving envelopes, When the developer runs `zornmesh tail <subject-pattern>`, Then matching envelopes are streamed in daemon sequence order, And non-matching envelopes are not emitted.
  > AC: Given the tail command runs in human mode, When matching events arrive, Then stdout shows readable event summaries with timestamp, subject, source, target or subscriber, delivery state, and correlation ID, And no secret payload values are displayed.
  > AC: Given the tail command runs with JSON output, When matching events arrive, Then stdout emits NDJSON with one stable structured event per line, And human prose, progress text, and ANSI escape codes are not mixed into the stream.
  > AC: Given the daemon disconnects, restarts, or falls behind during tailing, When the tail command detects the condition, Then it emits a stable disconnected/stale/backfill status according to output mode, And exits or reconnects according to documented behavior.
  > AC: Given live-tail tests run, When matching, non-matching, JSON/NDJSON, redacted payload, daemon disconnect, and backfill scenarios execute, Then output ordering and mode separation match fixtures.
  > Spec: specs/planning-artifacts/epics.md#story-4-7
- [ ] Story 4.8: Redeliver Previously Sent Envelopes Safely
  > As a developer recovering from a failed workflow
  > I want to redeliver a previously sent envelope from the audit log
  > So that I can recover work without manually reconstructing payloads or hiding that replay occurred.
  > AC: Given an envelope exists in the audit log and is eligible for redelivery, When the developer requests replay/redelivery for that envelope, Then the daemon creates a new delivery attempt linked to the original message, And the replay is clearly marked as replayed-from the original rather than treated as the original send.
  > AC: Given the selected envelope is ineligible for redelivery because of retention, authorization, payload size, redaction, or policy limits, When redelivery is requested, Then the command returns a stable refusal reason, And no new delivery attempt is created.
  > AC: Given redelivery is allowed, When the replayed envelope is routed, Then it receives a new message/delivery identity while preserving correlation and replay lineage metadata, And trace output can show original and replayed attempts together.
  > AC: Given a developer requests dry-run or preview behavior before redelivery, When the preview is generated, Then the output shows target, subject, safe payload summary, replay lineage, policy checks, expected effect, and required confirmation input, And no delivery side effect occurs.
  > AC: Given replay/redelivery would create a side effect, When the command runs in interactive, non-interactive, JSON/API, or scripted mode, Then redelivery requires explicit confirmation, `--yes`, or a preview-issued confirmation token according to documented mode rules, And missing, stale, or mismatched confirmation refuses replay without creating a delivery attempt.
  > AC: Given redelivery tests run, When eligible replay, ineligible replay, preview, confirmation required, `--yes`, stale confirmation token, non-interactive refusal, replay lineage, and redaction scenarios execute, Then replay behavior is deterministic, auditable, and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-4-8
- [ ] Story 4.9: Configure Retention and Surface Retention Gaps
  > As an operator
  > I want configurable retention for messages, dead letters, and audit records
  > So that local storage remains bounded while trace gaps are explicit and explainable.
  > AC: Given default retention settings are active, When messages, dead letters, and audit records age past their configured thresholds, Then retention jobs purge eligible records within the documented window, And purge actions are themselves observable as audit/retention events.
  > AC: Given retention purges audit entries from the middle of an audit hash chain, When purge work commits, Then purged rows are replaced by retention checkpoint/tombstone evidence containing sequence range, hash anchors, purge reason, and safe metadata, And offline verification can distinguish valid retention continuity from tampering without requiring raw purged payloads.
  > AC: Given an operator configures retention by age, count, or capability class, When the daemon starts or reloads supported config, Then valid settings are applied deterministically, And invalid settings are rejected with stable validation errors and no partial unsafe config.
  > AC: Given trace or inspect output references records removed by retention, When a developer queries the affected correlation ID or time window, Then the output marks a retention gap explicitly, And provides next-step guidance instead of returning misleading empty success.
  > AC: Given retention sweeps run while publishes, subscriptions, trace, or inspect operations are active, When purge work executes, Then active read/write operations are not blocked beyond the documented budget, And no unexpired record is removed.
  > AC: Given retention tests run, When default purge, configured purge, invalid config, retention gap, middle-chain purge, retention checkpoint, verify-after-retention, active read/write, and audit-of-purge scenarios execute, Then purging behavior is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-4-9
- [ ] Story 4.10: Verify Audit Log Tamper Evidence Offline
  > As a compliance-minded developer or operator
  > I want to verify the audit log hash chain without a running daemon
  > So that I can prove local evidence has not been silently modified.
  > AC: Given audit entries have been written by the daemon, When the operator runs offline audit verification against the local audit store, Then the verifier walks the audit hash chain and reports valid, tampered, incomplete, or unreadable status, And the command does not require daemon access.
  > AC: Given a single audit row is modified, removed, reordered, or replaced, When offline verification runs, Then verification detects the tamper condition and reports the first detected break with safe diagnostics, And the command exits with a stable verification-failed exit code.
  > AC: Given audit entries include redacted or personal-data-handling markers, When verification runs, Then redaction markers preserve chain verifiability, And raw secret values are not required or emitted by the verifier.
  > AC: Given audit retention checkpoints or tombstones exist, When offline verification walks the audit store, Then the verifier preserves hash-chain continuity across retained segments, And reports valid retention gaps separately from tamper, corruption, or missing data.
  > AC: Given the audit store is missing, locked, unreadable, or from an unsupported future schema, When verification runs, Then the command returns a stable structured error, And remediation text distinguishes missing data from tamper evidence.
  > AC: Given audit verification tests run, When valid chain, modified row, deleted row, reordered row, redacted chain, retention checkpoint, valid retention gap, missing store, and unsupported schema scenarios execute, Then offline verification behavior is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-4-10
### Compliance, Audit, and Release Trust Evidence
> Goal: Operators and compliance reviewers can verify release integrity, export evidence, prove audit-log integrity, handle redaction/deletion, and map events to required AI-risk/compliance frameworks.

- [ ] Story 5.1: Produce and Verify Release Signatures, SBOMs, and Reproducibility Evidence
  > As an operator
  > I want to verify the installed `zornmesh` artifact signature and retrieve its SBOM
  > So that I can trust what binary or SDK package is running in my local environment.
  > AC: Given the v0.1 release pipeline builds Linux and macOS artifacts and SDK packages, When release preflight runs, Then every artifact has a Sigstore signature, CycloneDX SBOM, dependency inventory, provenance metadata, and reproducibility report where the toolchain permits, And missing signature, missing SBOM, unaccounted dependency, or non-reproducible reference build status fails release readiness instead of being deferred to install-time verification.
  > AC: Given a signed `zornmesh` release artifact is installed, When the operator runs the release verification command or doctor check, Then the command verifies the artifact against the published Sigstore signature, And reports verified, unverifiable, missing-signature, or mismatch states with stable exit codes.
  > AC: Given the installed artifact has an associated CycloneDX SBOM, When the operator runs `zornmesh inspect sbom` or equivalent structured command, Then the SBOM is returned in the documented format, And JSON output can be consumed without human prose mixed into stdout.
  > AC: Given a source-built installation is used, When SBOM generation or lookup runs, Then the command reports whether the SBOM was generated at install/build time or is unavailable, And unavailable SBOM status is explicit rather than treated as success.
  > AC: Given signature or SBOM verification fails, When the operator inspects diagnostics, Then the output includes safe remediation guidance, And no network fetch or remote trust decision occurs unless explicitly configured by the operator.
  > AC: Given release-integrity tests run, When valid signature, missing signature, mismatched artifact, valid SBOM, missing SBOM, and JSON output scenarios execute, Then verification behavior is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-5-1
- [ ] Story 5.2: Enforce Compliance Traceability Fields on Envelopes
  > As a compliance reviewer
  > I want every evidence-bearing envelope to carry required traceability fields
  > So that agent actions can be mapped to who acted, what capability was used, and what prior message caused it.
  > AC: Given an agent sends, receives, replies, streams, ACKs, NACKs, replays, or triggers a dead-letter state, When the daemon records the envelope or related audit event, Then the record includes agent identity, capability or subject, timestamp, correlation ID, trace ID, and prior-message lineage where applicable, And missing required traceability fields produce stable validation or evidence-gap results.
  > AC: Given an envelope uses a capability descriptor, When the evidence record is written, Then the capability identifier and version are preserved in safe evidence metadata, And high-privilege capability decisions link to their authorization outcome.
  > AC: Given a traceability field contains sensitive data or references redacted payload material, When evidence is rendered or exported, Then raw sensitive values are redacted while stable identifiers and lineage remain verifiable.
  > AC: Given legacy, partial, or bridge-originated records cannot provide all fields, When compliance traceability validation runs, Then the record is marked with an explicit evidence-gap reason, And the system does not silently claim compliance completeness.
  > AC: Given compliance traceability tests run, When normal send, high-privilege invoke, replay, DLQ, MCP-bridge, missing-field, and redacted-field scenarios execute, Then traceability fields and evidence-gap behavior are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-5-2
- [ ] Story 5.3: Export Evidence Bundle for a Time Window
  > As a compliance reviewer
  > I want to export a self-contained evidence bundle for a time window
  > So that I can review local agent activity, release integrity, and configuration posture without manual file gathering.
  > AC: Given audit, trace, signature, SBOM, and configuration evidence exists for a requested time window, When the reviewer runs the evidence export command with `--since` and `--until`, Then the command emits a self-contained bundle containing audit-log slice, trace/correlation references, SBOM, signature verification status, and sanitized config snapshot, And the bundle includes a manifest describing included sections and evidence gaps.
  > AC: Given a 7-day evidence window is exported on the v0.1 reference machine, When audit export runs, Then the export completes within 5 minutes, And the manifest records duration and any performance-limit evidence gaps.
  > AC: Given the requested time window includes retained and purged data, When export runs, Then retained data is included and purged portions are marked as retention gaps, And the export does not represent missing records as complete evidence.
  > AC: Given evidence contains secrets or personal-data redaction markers, When the bundle is generated, Then raw secrets are not emitted, And redaction/proof markers remain sufficient for audit-chain and traceability review.
  > AC: Given export cannot complete because of unreadable store, invalid time window, missing release metadata, or unsupported schema, When the command fails, Then it returns a stable structured error, And no partial bundle is reported as complete.
  > AC: Given evidence export tests run, When complete export, incident-review export, release-review export, retention gap, redacted export, missing SBOM/signature, invalid time window, and store error scenarios execute, Then exported bundle content and manifest are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-5-3
- [ ] Story 5.4: Redact Personal Data While Preserving Audit Integrity
  > As a subject data owner or compliance reviewer
  > I want personal data referenced in envelopes to be redacted through a documented procedure
  > So that privacy obligations can be met without destroying audit-chain integrity.
  > AC: Given a documented redaction request identifies a subject, time window, correlation ID, or record scope, When the operator runs the redaction command, Then matching personal-data fields are replaced with redaction markers, And non-personal traceability fields such as correlation IDs, trace IDs, timestamps, and lineage remain available where policy permits.
  > AC: Given redaction affects audit-relevant records, When the redaction is applied, Then existing audit-chain entries and prior hashes are never rewritten, deleted, or re-linked, And redaction is represented by append-only tombstone/redaction-marker records, a durable scope checkpoint, and a `REDACTION_APPLIED` proof record referencing original record IDs/hashes, actor, timestamp, policy/version, and redaction scope., And offline audit verification validates chain continuity through the checkpoint/proof records and distinguishes authorized redaction from missing, deleted, reordered, or tampered rows.
  > AC: Given matching records are being written while a redaction request runs, When redaction begins, Then the operation establishes a durable cutoff/checkpoint for the redaction scope, And records at or before the checkpoint are redacted or explicitly refused atomically, while post-checkpoint matching records are blocked, queued for follow-up, or reported as requiring a subsequent redaction run., And no in-flight matching record can silently bypass redaction.
  > AC: Given the requested redaction scope is invalid, too broad, outside retention, or conflicts with immutable evidence policy, When redaction is requested, Then the command returns a stable refusal or evidence-gap result, And no partial redaction is reported as complete.
  > AC: Given redacted records are later inspected, traced, exported, or dead-lettered, When those surfaces render the records, Then redaction markers appear consistently, And raw personal data does not reappear from cached, indexed, or derived fields.
  > AC: Given redaction tests run, When valid redaction, invalid scope, retention gap, authorized redaction proof, unauthorized tamper attempt, concurrent matching write, checkpoint cutoff, post-checkpoint follow-up required, trace after redaction, export after redaction, and cache/index scenarios execute, Then redaction behavior is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-5-4
- [ ] Story 5.5: Map Envelopes to NIST AI RMF Functions and Categories
  > As a risk reviewer
  > I want local mesh events mapped to NIST AI RMF functions and categories
  > So that audits can connect concrete runtime evidence to recognized AI risk-management controls.
  > AC: Given envelopes, audit events, capability decisions, redaction events, and release evidence are persisted, When the reviewer runs the AI RMF mapping report, Then each included evidence type is mapped to the applicable Govern, Map, Measure, and Manage function/category references, And unmapped evidence is explicitly marked rather than silently omitted.
  > AC: Given an evidence record lacks required metadata for a confident AI RMF mapping, When the report is generated, Then the record is included with an evidence-gap reason, And the report does not claim full control coverage for that record.
  > AC: Given AI RMF mapping definitions are versioned, When the report is generated, Then the output records the mapping-definition version, schema version, generation time, and input evidence window, And prior fixtures remain reproducible across mapping-definition updates.
  > AC: Given an authorized reviewer needs to override an unmapped or incorrect AI RMF mapping, When the reviewer submits a manual override, Then the workflow requires actor identity, evidence reference, previous mapping, requested mapping, mapping-definition version, reason, timestamp, and review/expiry status, And the override validates that the target function/category exists without modifying the original evidence record.
  > AC: Given a manual AI RMF override is accepted, rejected, expired, or superseded, When audit evidence is persisted, Then an append-only audit record captures actor/session, source evidence ID, before/after mapping, reason, mapping-definition version, decision outcome, and timestamp, And reports distinguish automatic mappings, manual overrides, unmapped evidence, and evidence-gap records.
  > AC: Given the report is exported as part of a compliance bundle, When the evidence bundle is opened offline, Then AI RMF mappings, evidence gaps, and source trace references are reviewable without network access, And redacted records preserve mapping context without exposing raw protected data.
  > AC: Given AI RMF mapping tests run, When complete coverage, unmapped evidence, missing metadata, mapping-version drift, redacted records, manual override accepted, manual override rejected, override audit log, override version drift, and offline bundle scenarios execute, Then mapping output is deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-5-5
### Local Web Control Room and Safe Intervention
> Goal: Developers can open the local UI, observe connected agents, inspect trace chronology, send direct/broadcast messages safely, confirm outcomes, reconnect/backfill, and copy CLI handoffs. **Dependency gate:** Epic 6 is not implementation-ready, and Stories 6.2-6.9 must not begin, until Story 6.1 (a) verifies and references — by section — the existing v0.1 local UI architecture amendment that already supersedes earlier no-GUI/frontend/static-asset text, (b) pins the local UI framework wording so no Node-served runtime, hosted serving model, or remote-asset dependency can enter v0.1, and (c) scaffolds the local web app shell, shared UI/API taxonomies, component fixture baseline, and scope-boundary checks against that referenced architecture.

- [ ] Story 6.1: Verify Local UI Architecture, Pin Framework Wording, and Scaffold Local Web App Shell
  > As a developer
  > I want Story 6.1 to verify the existing local UI architecture amendment, pin the framework wording, and scaffold the local web app shell before feature work begins
  > So that implementation follows the validated PRD/UX/architecture scope and v0.1 cannot silently introduce a Node-served runtime, hosted serving model, or remote-asset dependency.
  > AC: Given the architecture artifact already contains the v0.1 local UI amendment that supersedes earlier no-GUI/frontend/static-asset text, When Story 6.1 is completed, Then Story 6.1 cites the existing amendment by section reference (architecture supersession note, Local UI scope decision, and the local web companion UI section) and links Epic 6 planning to those sections, And Story 6.1 pins the v0.1 local UI framework wording to "Bun-managed React app, locally bundled and offline-served by the daemon UI gateway on loopback only," and records that v0.1 ships no Node-served runtime, no hosted serving model, no Next.js server features, and no remote browser assets, And Story 6.1 records that hosted/cloud dashboard, LAN/public console, accounts/teams, full chat workspace, workflow editor, remote browser assets, and external runtime services remain out of scope, consistent with NFR-S8, NFR-S10, NFR-S11, and NFR-C7.
  > AC: Given the local UI shell is scaffolded, When the developer runs the documented UI build/test entrypoint, Then a Bun-managed React app shell exists for the `zornmesh ui` surface, produces statically bundled assets that are served only by the daemon UI gateway on loopback, and introduces no Node-served runtime, no Next.js server features, no remote-asset dependency, and no external runtime service, And Tailwind-aligned styling, Radix-style accessible primitive composition, and project-owned UI primitive wrappers are established without adding unsupported package managers.
  > AC: Given foundational UI tokens are defined, When the shell renders fixture states, Then dark-first graphite/charcoal, electric-blue actions, cyan local-trust accents, semantic success/warning/error/neutral states, typography, spacing, radius, borders, focus rings, and light-mode support are available through project-owned tokens, And technical strings such as agent IDs, trace IDs, subjects, timestamps, CLI commands, and payload metadata use readable monospace styling.
  > AC: Given shared state language exists across CLI, SDK, daemon, and UI surfaces, When the UI shell renders initial fixture data, Then agent status, delivery state, trace completeness, daemon health, and trust posture taxonomies are represented as shared UX/API contracts, And unknown or future states render explicit fallback labels rather than silent success states.
  > AC: Given UI component fixtures are seeded before full daemon integration, When the fixture test suite runs, Then buttons, inputs, dialogs, popovers, tooltips, tabs, menus, toasts, badges, panels, and layout primitives render deterministic baseline, loading, error, disabled, focus, and reduced-motion states, And fixture failures point to the affected component/state contract before dependent UI feature stories proceed.
  > AC: Given UI routes, navigation, and actions are scaffolded, When scope-boundary checks run, Then the shell exposes only observe, inspect, reconnect/backfill, safe direct send, safe broadcast, outcome review, and CLI handoff surfaces, And workflow editing, full chat orchestration, cloud sync, LAN/public serving, account/team management, and remote dashboard behavior are absent or return explicit out-of-scope errors.
  > Spec: specs/planning-artifacts/epics.md#story-6-1
- [ ] Story 6.2: Launch Protected Loopback UI with Offline Assets
  > As a developer
  > I want `zornmesh ui` to launch a protected local web UI
  > So that I can inspect and operate the mesh from a browser without exposing the control surface beyond my machine.
  > AC: Given the local daemon is available and the UI feature is enabled, When the developer runs `zornmesh ui`, Then the command starts or connects to the local UI server on loopback only, And it either opens a browser window or prints a protected loopback URL suitable for copy/paste.
  > AC: Given the preferred UI port is already bound by another process, When `zornmesh ui` starts, Then the command either selects a documented alternate loopback port or fails with a stable `UI_PORT_IN_USE` error and remediation, And it never sends session tokens to, proxies through, or treats the existing process as trusted.
  > AC: Given the browser opens the local UI URL, When the session is established, Then access requires a per-launch high-entropy session token or one-time code with bounded lifetime, server-side revocation on shutdown/expiry, and no persistence in localStorage, And token-bearing material is removed from browser history after exchange, omitted from logs/audit payloads/CLI handoff text, protected with `Referrer-Policy: no-referrer`, and not leaked through referrer headers.
  > AC: Given browser requests reach the UI API or live event transport, When HTTP, WebSocket, or SSE requests are made, Then CORS denies by default except the exact loopback origin, Origin/Host checks fail closed, and WebSocket/SSE upgrades require the same session protection as HTTP, And state-changing requests require CSRF protection bound to the server-side session and derive actor/session identity on the server rather than trusting browser-supplied actor fields.
  > AC: Given UI assets are served, When the browser loads the app, Then JavaScript, CSS, fonts, icons, and fixture assets are bundled for offline use, And the app makes no external browser network requests for runtime assets, telemetry, fonts, or analytics.
  > AC: Given the daemon, session, or local trust state changes, When the UI shell renders status chrome, Then it displays daemon health, loopback-only status, session protection, socket path, schema version, bundled/offline asset indicator, and stale/disconnected/session-expired warnings, And critical status is communicated with text/icon/shape, not color alone.
  > AC: Given launch and security tests run, When open-browser, printed-URL, port in use, invalid token, missing token, token expiry, token history cleanup, referrer leak prevention, unsafe origin, CORS rejection, CSRF failure, WebSocket/SSE unauthenticated upgrade, actor/session binding, non-loopback bind attempt, offline asset, and daemon-unavailable scenarios execute, Then launch behavior and failures are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-2
- [ ] Story 6.3: Render Live Agent Roster and Local Trust Status
  > As a developer
  > I want the Live Mesh Control Room to show connected agents and daemon trust state
  > So that I can quickly understand who is participating in the local mesh and whether the control room is safe to use.
  > AC: Given registered agents and capability summaries are available from the daemon, When the Live Mesh Control Room loads, Then the roster shows each agent's display name, stable ID, status, transport/source, capability summary, last-seen recency, recent activity count, and warning markers, And MCP stdio, native SDK, stale, errored, disconnected, and reconnecting states are visibly distinct.
  > AC: Given a developer selects an agent, When the agent detail or capability card opens, Then it shows identity, transport, capabilities, subscriptions, recent traces, activity, trust indicators, permission indicators, and high-privilege warnings, And unavailable or denied high-privilege capabilities are explained without enabling unsafe actions.
  > AC: Given the roster has many agents or mixed states, When the developer searches, filters, or highlights agents by ID, name, capability, status, warning, source, or recent trace, Then matching agents remain findable without changing message chronology, And active filters are visible and removable.
  > AC: Given roster or daemon state is empty, loading, stale, degraded, unavailable, or session-expired, When the control room renders, Then persistent state panels explain the condition and next action, And transient toasts never replace persistent status for critical trust or availability issues.
  > AC: Given roster fixture tests run, When empty roster, active agents, stale agents, disconnected agents, MCP/native source, high-privilege warning, filtered roster, unavailable daemon, and session-expired scenarios execute, Then roster and trust-state rendering are deterministic and fixture-covered.
  > AC: Given the 3-agent roster fixture runs after daemon readiness, When the Live Mesh Control Room renders connected agents, Then agent roster and local trust status are visible within 2 seconds on the v0.1 reference browser profile, And failures emit stable UI performance evidence.
  > Spec: specs/planning-artifacts/epics.md#story-6-3
- [ ] Story 6.4: Render Daemon-Sequence Timeline and Event Detail Panel
  > As a developer
  > I want a daemon-sequenced trace timeline with event details
  > So that I can understand message flow and delivery state without stitching logs together manually.
  > AC: Given trace and message events are available from the daemon, When the control room renders the timeline, Then events are ordered by daemon sequence as the primary chronology, And browser receipt time appears only as secondary diagnostic metadata.
  > AC: Given timeline events include causality and delivery metadata, When events render, Then each row shows event summary, sender/recipient, subject or operation, daemon sequence, timestamp, causal marker, delivery state badge, keyboard selection, and expansion/selection affordance, And pending, queued, accepted, delivered, acknowledged, rejected, failed, cancelled, replayed, dead-lettered, stale, and unknown states use consistent labels.
  > AC: Given a developer selects a trace event, When the detail panel opens, Then it shows event summary, sender, recipients, subject, correlation ID, daemon sequence, timestamp, parent/child links, payload metadata, delivery outcome, timing, source/target agent, suggested next action, and copyable relevant command where available, And selected detail remains stable while new live events arrive.
  > AC: Given a trace exceeds the browser rendering or memory budget, When the timeline loads, Then the UI requests daemon-sequence pages/windows and renders a virtualized timeline rather than materializing the full trace in the DOM, And loaded range, total/unknown count, gaps, and partial-window state are visible without changing daemon-sequence ordering.
  > AC: Given a trace contains <= 500 events, When the timeline and selected-event detail render on the reference browser profile, Then ordered timeline and selected detail are visible within 1 second, And performance evidence is recorded with the fixture result.
  > AC: Given late, reconstructed, replayed, missing-parent, gap, or dead-letter states appear, When the timeline and detail panel render those events, Then each state is labeled with accessible text and a recovery/inspection cue, And the UI does not imply a complete trace when evidence gaps exist.
  > AC: Given timeline/detail tests run, When complete trace, partial trace, missing parent, late event, replayed event, dead letter, keyboard navigation, live append, selected-detail stability, large trace virtualization, paged timeline window, partial-window gap markers, and unknown delivery state scenarios execute, Then timeline ordering and detail rendering are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-4
- [ ] Story 6.5: Open Focused Trace Reader with CLI Handoff Commands
  > As a developer debugging a multi-agent conversation
  > I want to open a focused trace reader for one correlation ID and copy matching CLI handoff commands
  > So that I can move between visual inspection and terminal investigation without losing context.
  > AC: Given a trace event has a correlation ID, When the developer opens the focused trace reader, Then the UI shows the full known conversation when it is within the browser window budget, or a clearly labeled daemon-sequence window with load-more controls when it is too large, And parent/child links, caused-by/responds-to/replayed-from/broadcast fan-out labels, delivery states, payload summaries, timing, and focus/pause behavior preserve daemon-sequence ordering.
  > AC: Given trace evidence is incomplete, reconstructed, stale, dead-lettered, or has missing parents, When the focused trace reader renders, Then it labels the gap or recovery state in accessible text, And it provides a guided recovery cue such as inspect trace, inspect dead letter, replay, reconnect, or audit verification where applicable.
  > AC: Given the current context supports terminal follow-up, When the developer opens a CLI command copy block, Then commands for trace, inspect, replay, agents, doctor, and audit operations include context-preserving arguments such as correlation ID, agent ID, daemon sequence, time window, or evidence path, And each command includes a description, expected outcome, copy action, and copied feedback.
  > AC: Given context values contain spaces, quotes, semicolons, backticks, dollar signs, newlines, glob characters, option-like prefixes, or shell metacharacters, When CLI handoff commands are generated, Then commands are constructed from argv tokens with documented shell quoting/escaping and never by unsafe string concatenation, And copied commands cannot introduce command substitution, redirection, chaining, environment assignment, or extra arguments from untrusted trace, agent, subject, or evidence values.
  > AC: Given a command is unavailable because the daemon is offline, audit evidence is missing, context is insufficient, or the operation would be unsafe, When the command block renders, Then it shows an unavailable/requires-daemon/offline-audit explanation, And no misleading command is offered.
  > AC: Given focused trace tests run, When complete trace, missing trace, reconstructed trace, broadcast fan-out, dead-letter event, command copy, unavailable command, large focused trace, shell metacharacter escaping, option-like IDs, newline-bearing context values, pause/focus, and return-to-control-room scenarios execute, Then the focused reader and CLI handoff behavior are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-5
- [ ] Story 6.6: Send Safe Direct Messages from the UI
  > As a developer
  > I want to send a direct message to one selected agent only after reviewing target identity and payload preview
  > So that human-originated UI sends are intentional, validated, and auditable.
  > AC: Given a developer selects one target agent, When the safe composer opens in direct mode, Then it shows target display name, stable agent ID, transport/source, status, capability summary, high-privilege warnings, message body, subject/operation where applicable, payload preview, and validation state, And direct mode is visually and textually distinct from broadcast mode.
  > AC: Given the target is stale, disconnected, missing required capability, denied by allowlist, unavailable because the daemon is offline, or the message body/subject is invalid, When the developer attempts to send, Then the send action is blocked with an explanatory validation error, And disabled actions explain why without requiring hover-only affordances.
  > AC: Given the direct message is valid, When the developer sends it, Then the UI prevents duplicate submission while pending and displays queued, delivered, acknowledged, rejected, timed-out, or dead-lettered outcome states as they arrive, And persistent outcome display is not replaced by a transient toast.
  > AC: Given a valid direct send targets one available recipient, When the daemon accepts the send, Then terminal delivery outcome is displayed within 5 seconds unless the explicit agent timeout policy exceeds that budget, And over-budget pending state remains visible with the active timeout policy.
  > AC: Given the direct send is accepted by the daemon, When audit evidence is persisted, Then the record links actor/session, target recipient, trace/correlation ID, payload summary, validation outcome, and delivery outcome, And raw secrets or protected payload fields are not exposed in UI audit summaries.
  > AC: Given direct-send tests run, When valid send, invalid recipient, stale recipient, denied capability, empty body, invalid subject, daemon unavailable, duplicate click, rejected send, timed-out send, dead-lettered send, and audit-link scenarios execute, Then direct composer behavior and audit linkage are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-6
- [ ] Story 6.7: Confirm Broadcast Scope and Show Per-Recipient Outcomes
  > As a developer
  > I want broadcasts to require explicit scope review and show per-recipient outcomes
  > So that broad-impact sends are deliberate and failures are visible for each target.
  > AC: Given the developer chooses broadcast mode, When recipients are selected or resolved from capability/status filters, Then the UI shows recipient count, included recipients, excluded or incompatible recipients, payload summary, capability warnings, and unsafe-scope warnings, And broadcast mode is visually and textually distinct from direct mode.
  > AC: Given a broadcast would affect multiple recipients, When the developer attempts to send, Then a confirmation dialog names the scope, recipient count, excluded/incompatible recipients, and payload summary, And the final confirmation requires explicit user action and avoids modal chains.
  > AC: Given recipient membership, capability compatibility, or agent status changes after the confirmation preview is shown, When the developer confirms the broadcast, Then the daemon validates the confirmed recipient snapshot/revision before accepting the send, And any recipient-set drift blocks the send, refreshes the preview, and requires explicit reconfirmation.
  > AC: Given the confirmed broadcast is sent, When delivery outcomes arrive, Then the UI displays a persistent per-recipient outcome list with queued, delivered, acknowledged, rejected, timed-out, dead-lettered, pending, stale, partial-success, all-failed, and success states as applicable, And failure reasons, timing, retry affordance, and inspect affordance are available per recipient where safe.
  > AC: Given a valid broadcast targets three recipients, When the daemon accepts the send, Then terminal per-recipient outcomes are displayed within 5 seconds unless explicit agent timeout policy exceeds that budget, And pending or partial states remain visible until every recipient reaches a terminal or policy-defined timeout state.
  > AC: Given the broadcast is accepted by the daemon, When audit evidence is persisted, Then the record links actor/session, requested recipient scope, previewed recipient snapshot, accepted recipient snapshot/revision, actual recipient list, excluded recipients, drift/reconfirmation outcome, trace/correlation ID, payload summary, and per-recipient delivery outcomes, And partial failure remains visible in both UI and audit evidence.
  > AC: Given broadcast tests run, When successful broadcast, partial failure, all failed, excluded recipients, incompatible recipients, stale recipients, unsafe scope, confirmation cancel, recipient drift after preview, reconfirmation required, stale snapshot rejected, duplicate submit, retry/inspect affordance, and audit-link scenarios execute, Then broadcast confirmation and per-recipient outcome behavior are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-7
- [ ] Story 6.8: Reconnect, Backfill, and Preserve UI Context
  > As a developer
  > I want the UI to recover after refreshes, reconnects, and daemon restarts
  > So that I can continue investigating the same trace without losing chronology or mistaking gaps for complete evidence.
  > AC: Given the developer refreshes the browser or the UI reconnects after a transient disconnect, When the session is still valid, Then the UI restores the selected trace, selected agent where possible, active filters, and current view mode after daemon backfill completes, And restored events remain ordered by daemon sequence.
  > AC: Given the daemon restarts, becomes unavailable, reconnects, or reports a schema/session change, When the UI detects the transition, Then persistent status chrome shows starting, reconnecting, degraded, unavailable, stale, schema-mismatch, or session-expired state as applicable, And actions that cannot safely run are disabled with explanatory text.
  > AC: Given backfill includes retained, purged, late, duplicated, or reconstructed events, When the UI merges live and backfilled data, Then it deduplicates by stable daemon/event identity, marks retention gaps and late/reconstructed events, and preserves selected detail stability, And the UI never reorders trace events by browser receipt time.
  > AC: Given reconnect/backfill would exceed browser capacity, When the UI restores context, Then it restores the selected trace as a daemon-sequence window around the prior selection and marks the view partial until additional pages load, And actions and status copy do not imply the entire trace is loaded.
  > AC: Given reconnect or backfill cannot complete, When the developer views the affected trace or roster state, Then the UI shows an evidence-gap or recovery panel with safe next actions such as retry reconnect, inspect trace by CLI, inspect daemon health, or export audit evidence, And partial state remains visible rather than being replaced by a generic failure page.
  > AC: Given reconnect/backfill tests run, When browser refresh, transient disconnect, daemon restart, unavailable daemon, schema mismatch, expired session, retained plus purged data, late event, duplicate event, selected-detail stability, and failed backfill scenarios execute, Then reconnect behavior and context preservation are deterministic and fixture-covered.
  > Spec: specs/planning-artifacts/epics.md#story-6-8
- [ ] Story 6.9: Prove Accessibility, Responsive Behavior, and Browser Fixture Coverage
  > As a developer or QA reviewer
  > I want the local web UI's critical journeys verified across accessibility, responsive layouts, and supported browsers
  > So that the control room remains usable and trustworthy for real debugging sessions.
  > AC: Given the local UI critical journeys exist for roster inspection, trace timeline navigation, trace detail, focused trace reader, direct send, broadcast confirmation, outcome review, reconnect/backfill, and CLI command copy, When accessibility checks run, Then automated checks, keyboard-only walkthroughs, focus-order checks, screen-reader spot checks, reduced-motion checks, color-blindness checks, and no-color-only verification pass for those journeys, And failures identify the affected journey and component state.
  > AC: Given the UI runs at mobile, tablet, desktop, and wide desktop breakpoints, When responsive fixture tests execute, Then the layout uses one-pane mobile, two-pane tablet, and three-pane desktop behavior as specified, And no responsive mode reorders trace events, hides delivery/failure state, or loses selected detail stability.
  > AC: Given technical data contains long agent IDs, correlation IDs, subjects, timestamps, payload summaries, CLI commands, and error messages, When the UI renders across supported breakpoints, Then text remains readable, copyable where appropriate, and does not break timeline chronology, controls, or status visibility, And truncation or wrapping preserves accessible labels.
  > AC: Given browser E2E coverage runs for supported local browsers, When Chromium, Firefox, and WebKit/Safari-compatible scenarios execute, Then fixture-backed journeys for complete, partial, missing, reconstructed, and live traces; stale/disconnected agents; daemon unavailable; session expired; direct send; broadcast success; broadcast partial failure; validation-blocked send; late event arrival; and backfill are covered, And browser differences are represented as explicit unsupported-state or defect evidence rather than ignored.
  > AC: Given browser fixture evidence is collected for current stable Chromium, Firefox, and Safari/WebKit, When a browser-specific behavior differs or fails, Then the fixture records explicit unsupported-state or defect evidence, And cross-browser gaps cannot be silently ignored in release readiness.
  > AC: Given release/readiness checks include UI quality gates, When the UI test suite completes, Then accessibility, responsive, browser, offline-asset, and critical-journey fixture results are emitted as stable evidence for implementation readiness, And the suite fails explicitly when required browser or accessibility tooling is unavailable.
  > Spec: specs/planning-artifacts/epics.md#story-6-9

## Completed

## Notes
- Follow TDD methodology (red-green-refactor)
- One story per Ralph loop iteration
- Update this file after completing each story
