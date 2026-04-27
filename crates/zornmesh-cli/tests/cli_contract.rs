use std::{
    fs,
    io::Write,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use zornmesh_core::Envelope;
use zornmesh_store::{
    DeadLetterFailureCategory, EvidenceDeadLetterInput, EvidenceEnvelopeInput,
    EvidenceStateTransitionInput, EvidenceStore, FileEvidenceStore,
};

const TEST_SOCKET: &str = "/tmp/zorn-cli-contract.sock";

fn zornmesh(args: &[&str]) -> Output {
    zornmesh_command(args)
        .output()
        .expect("zornmesh binary runs")
}

fn zornmesh_command(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zornmesh"));
    command
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("ZORN_SOCKET_PATH")
        .env_remove("ZORN_EVIDENCE_PATH")
        .env_remove("XDG_RUNTIME_DIR");
    command
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf8")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got status {:?}, stderr {:?}",
        output.status.code(),
        stderr(output)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr must stay empty on success"
    );
}

fn assert_no_ansi(text: &str) {
    assert!(
        !text.contains("\u{1b}["),
        "output must not contain ANSI escapes"
    );
}

fn read_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout is valid JSON")
}

fn assert_read_json_contract(value: &serde_json::Value, command: &str) {
    let object = value.as_object().expect("read response is a JSON object");
    assert_eq!(object.len(), 5);
    for key in ["schema_version", "command", "status", "data", "warnings"] {
        assert!(object.contains_key(key), "missing top-level key {key}");
    }
    assert_eq!(value["schema_version"], "zornmesh.cli.read.v1");
    assert_eq!(value["command"], command);
    assert_eq!(value["status"], "ok");
    assert!(value["data"].is_object(), "data must be an object");
    assert!(value["warnings"].is_array(), "warnings must be an array");
}

fn unique_socket(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let short_name: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect();
    PathBuf::from("/tmp")
        .join(format!("zm{short_name}-{}-{id}", std::process::id()))
        .join("z")
}

fn healthy_socket(name: &str) -> (UnixListener, PathBuf) {
    let path = unique_socket(name);
    let parent = path.parent().expect("socket path has parent");
    fs::create_dir_all(parent).expect("socket parent created");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).expect("socket parent secured");
    let listener = UnixListener::bind(&path).expect("socket listener binds");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("socket secured");
    (listener, path)
}

fn temp_config(contents: &str) -> std::path::PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("zornmesh-cli-contract-{}-{id}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp config dir created");
    let path = dir.join("zornmesh.conf");
    fs::write(&path, contents).expect("temp config written");
    path
}

fn temp_evidence_path(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "zornmesh-cli-evidence-{name}-{}-{id}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("temp evidence dir created");
    dir.join("evidence.log")
}

fn evidence_envelope(
    source: &str,
    subject: &str,
    correlation_id: &str,
    timestamp_unix_ms: u64,
) -> Envelope {
    Envelope::with_metadata(
        source,
        subject,
        b"{\"password\":\"must-not-leak\"}".to_vec(),
        timestamp_unix_ms,
        correlation_id,
        "application/json; token=must-not-leak",
    )
    .expect("valid evidence envelope")
}

fn trace_envelope(
    source: &str,
    subject: &str,
    correlation_id: &str,
    timestamp_unix_ms: u64,
    span_id: &str,
) -> Envelope {
    Envelope::with_trace_context(
        source,
        subject,
        b"{\"secret\":\"must-not-leak\"}".to_vec(),
        timestamp_unix_ms,
        correlation_id,
        "application/json; token=must-not-leak",
        format!("00-4bf92f3577b34da6a3ce929d0e0e4736-{span_id}-01"),
        None,
    )
    .expect("valid trace envelope")
}

fn seed_inspect_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("evidence store opens");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.inspect.created",
                    "corr-inspect",
                    1_700_000_000_001,
                ),
                "msg-inspect-1",
                "trace-inspect",
                "accepted",
            )
            .expect("valid envelope input")
            .with_target("agent.local/target"),
        )
        .expect("first envelope persists");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.inspect.created",
                    "corr-inspect",
                    1_700_000_000_002,
                ),
                "msg-inspect-2",
                "trace-inspect",
                "accepted",
            )
            .expect("valid envelope input")
            .with_target("agent.local/target"),
        )
        .expect("second envelope persists");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/other",
                    "mesh.other.created",
                    "corr-other",
                    1_700_000_000_003,
                ),
                "msg-other",
                "trace-other",
                "accepted",
            )
            .expect("valid envelope input"),
        )
        .expect("other envelope persists");
    store
        .persist_dead_letter(
            EvidenceDeadLetterInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.inspect.created",
                    "corr-inspect",
                    1_700_000_000_001,
                ),
                "msg-inspect-1",
                "trace-inspect",
                "dead_lettered",
                DeadLetterFailureCategory::RetryExhausted,
                "retry exhausted with token=must-not-leak",
            )
            .expect("valid dead letter input")
            .with_intended_target("agent.local/target")
            .with_attempt_count(3)
            .with_last_failure_category(DeadLetterFailureCategory::Timeout)
            .with_timing(1_700_000_000_010, 1_700_000_000_020, 1_700_000_000_030),
        )
        .expect("dead letter persists");
}

struct TransitionSeed<'a> {
    daemon_sequence: u64,
    message_id: &'a str,
    actor: &'a str,
    action: &'a str,
    subject: &'a str,
    correlation_id: &'a str,
    trace_id: &'a str,
    state_from: &'a str,
    state_to: &'a str,
    details: &'a str,
}

fn transition(store: &FileEvidenceStore, seed: TransitionSeed<'_>) {
    store
        .persist_state_transition(
            EvidenceStateTransitionInput::new(
                seed.daemon_sequence,
                seed.message_id,
                seed.actor,
                seed.action,
                seed.subject,
                seed.correlation_id,
                seed.trace_id,
                seed.state_from,
                seed.state_to,
                seed.details,
            )
            .expect("valid transition input"),
        )
        .expect("state transition persists");
}

fn seed_trace_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("trace evidence store opens");
    let request = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/client",
                    "mesh.request.created",
                    "corr-trace",
                    1_700_000_000_100,
                    "1111111111111111",
                ),
                "msg-request",
                "trace-root",
                "accepted",
            )
            .expect("valid request input")
            .with_target("agent.local/worker"),
        )
        .expect("request persists");
    transition(
        &store,
        TransitionSeed {
            daemon_sequence: request.envelope().daemon_sequence(),
            message_id: "msg-request",
            actor: "agent.local/worker",
            action: "delivery_ack",
            subject: "mesh.request.created",
            correlation_id: "corr-trace",
            trace_id: "trace-root",
            state_from: "accepted",
            state_to: "acknowledged",
            details: "delivered to worker",
        },
    );

    let retry = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/client",
                    "mesh.request.retry",
                    "corr-trace",
                    1_700_000_000_110,
                    "2222222222222222",
                ),
                "msg-retry",
                "trace-root",
                "retrying",
            )
            .expect("valid retry input")
            .with_target("agent.local/worker")
            .with_parent_message_id("msg-request"),
        )
        .expect("retry persists");
    transition(
        &store,
        TransitionSeed {
            daemon_sequence: retry.envelope().daemon_sequence(),
            message_id: "msg-retry",
            actor: "agent.local/worker",
            action: "retry_scheduled",
            subject: "mesh.request.retry",
            correlation_id: "corr-trace",
            trace_id: "trace-root",
            state_from: "accepted",
            state_to: "retrying",
            details: "retry after transient timeout",
        },
    );

    let replay = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/client",
                    "mesh.request.replay",
                    "corr-trace",
                    1_700_000_000_120,
                    "3333333333333333",
                ),
                "msg-replay",
                "trace-root",
                "replayed",
            )
            .expect("valid replay input")
            .with_target("agent.local/worker")
            .with_parent_message_id("msg-request"),
        )
        .expect("replay persists");
    transition(
        &store,
        TransitionSeed {
            daemon_sequence: replay.envelope().daemon_sequence(),
            message_id: "msg-replay",
            actor: "agent.local/worker",
            action: "replay_enqueued",
            subject: "mesh.request.replay",
            correlation_id: "corr-trace",
            trace_id: "trace-root",
            state_from: "accepted",
            state_to: "replayed",
            details: "replayed from msg-request",
        },
    );

    let cancelled = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/client",
                    "mesh.request.cancel",
                    "corr-trace",
                    1_700_000_000_130,
                    "4444444444444444",
                ),
                "msg-cancel",
                "trace-root",
                "accepted",
            )
            .expect("valid cancellation input")
            .with_target("agent.local/worker")
            .with_parent_message_id("msg-request"),
        )
        .expect("cancellation persists");
    transition(
        &store,
        TransitionSeed {
            daemon_sequence: cancelled.envelope().daemon_sequence(),
            message_id: "msg-cancel",
            actor: "agent.local/client",
            action: "cancellation",
            subject: "mesh.request.cancel",
            correlation_id: "corr-trace",
            trace_id: "trace-root",
            state_from: "accepted",
            state_to: "cancelled",
            details: "cancelled by sender",
        },
    );

    let late = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/worker",
                    "mesh.reply.created",
                    "corr-trace",
                    1_700_000_000_090,
                    "5555555555555555",
                ),
                "msg-late-reply",
                "trace-root",
                "accepted",
            )
            .expect("valid late reply input")
            .with_target("agent.local/client")
            .with_parent_message_id("msg-request"),
        )
        .expect("late reply persists");
    transition(
        &store,
        TransitionSeed {
            daemon_sequence: late.envelope().daemon_sequence(),
            message_id: "msg-late-reply",
            actor: "agent.local/worker",
            action: "late_arrival",
            subject: "mesh.reply.created",
            correlation_id: "corr-trace",
            trace_id: "trace-root",
            state_from: "accepted",
            state_to: "late_arrival",
            details: "reply arrived after cancellation",
        },
    );

    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/worker",
                    "mesh.work.created",
                    "corr-trace",
                    1_700_000_000_140,
                    "6666666666666666",
                ),
                "msg-dlq",
                "trace-root",
                "accepted",
            )
            .expect("valid dead-letter input")
            .with_target("agent.local/client")
            .with_parent_message_id("msg-request"),
        )
        .expect("dead-letter envelope persists");
    store
        .persist_dead_letter(
            EvidenceDeadLetterInput::new(
                trace_envelope(
                    "agent.local/worker",
                    "mesh.work.created",
                    "corr-trace",
                    1_700_000_000_140,
                    "6666666666666666",
                ),
                "msg-dlq",
                "trace-root",
                "dead_lettered",
                DeadLetterFailureCategory::RetryExhausted,
                "retry exhausted with token=must-not-leak",
            )
            .expect("valid dead letter trace input")
            .with_intended_target("agent.local/client")
            .with_attempt_count(3)
            .with_last_failure_category(DeadLetterFailureCategory::Timeout)
            .with_timing(1_700_000_000_141, 1_700_000_000_151, 1_700_000_000_161),
        )
        .expect("trace dead letter persists");
}

fn seed_partial_trace_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("partial trace evidence store opens");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/orphan",
                    "mesh.orphan.created",
                    "corr-partial",
                    1_700_000_001_000,
                    "7777777777777777",
                ),
                "msg-orphan",
                "trace-partial",
                "accepted",
            )
            .expect("valid orphan input")
            .with_target("agent.local/target")
            .with_parent_message_id("msg-missing-parent"),
        )
        .expect("orphan persists");
}

fn seed_span_tree_streaming_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("span tree evidence store opens");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                trace_envelope(
                    "agent.local/client",
                    "mesh.request.created",
                    "corr-span-tree",
                    1_700_000_002_000,
                    "aaaaaaaaaaaaaaaa",
                ),
                "msg-root-request",
                "trace-span-tree",
                "accepted",
            )
            .expect("valid root request input")
            .with_target("agent.local/router"),
        )
        .expect("root request persists");

    for (message_id, subject, timestamp, span_id, state) in [
        (
            "msg-stream-001",
            "mesh.stream.chunk.001",
            1_700_000_002_010,
            "bbbbbbbbbbbbbbbb",
            "stream_continue",
        ),
        (
            "msg-stream-002",
            "mesh.stream.chunk.002",
            1_700_000_002_020,
            "cccccccccccccccc",
            "stream_continue",
        ),
        (
            "msg-stream-003",
            "mesh.stream.final",
            1_700_000_002_030,
            "dddddddddddddddd",
            "stream_final",
        ),
        (
            "msg-stream-cancelled",
            "mesh.stream.cancelled",
            1_700_000_002_040,
            "eeeeeeeeeeeeeeee",
            "stream_cancelled",
        ),
        (
            "msg-stream-failed",
            "mesh.stream.failed",
            1_700_000_002_050,
            "ffffffffffffffff",
            "stream_failed",
        ),
        (
            "msg-stream-gap",
            "mesh.stream.gap",
            1_700_000_002_060,
            "1111111111111110",
            "stream_gap",
        ),
    ] {
        store
            .persist_accepted_envelope(
                EvidenceEnvelopeInput::new(
                    trace_envelope(
                        "agent.local/router",
                        subject,
                        "corr-span-tree",
                        timestamp,
                        span_id,
                    ),
                    message_id,
                    "trace-span-tree",
                    state,
                )
                .expect("valid stream input")
                .with_target("agent.local/client")
                .with_parent_message_id("msg-root-request"),
            )
            .expect("stream record persists");
    }

    for (message_id, subject, timestamp, span_id, state) in [
        (
            "msg-fanout-alpha",
            "mesh.task.alpha",
            1_700_000_002_070,
            "1111111111111111",
            "accepted",
        ),
        (
            "msg-fanout-beta",
            "mesh.task.beta",
            1_700_000_002_080,
            "2222222222222222",
            "accepted",
        ),
        (
            "msg-retry-branch",
            "mesh.request.retry",
            1_700_000_002_090,
            "3333333333333333",
            "retrying",
        ),
        (
            "msg-replay-branch",
            "mesh.request.replay",
            1_700_000_002_100,
            "4444444444444444",
            "replayed",
        ),
        (
            "msg-reply",
            "mesh.reply.created",
            1_700_000_002_110,
            "5555555555555555",
            "accepted",
        ),
        (
            "msg-dlq-branch",
            "mesh.task.failed",
            1_700_000_002_120,
            "6666666666666666",
            "accepted",
        ),
    ] {
        store
            .persist_accepted_envelope(
                EvidenceEnvelopeInput::new(
                    trace_envelope(
                        "agent.local/router",
                        subject,
                        "corr-span-tree",
                        timestamp,
                        span_id,
                    ),
                    message_id,
                    "trace-span-tree",
                    state,
                )
                .expect("valid branch input")
                .with_target("agent.local/client")
                .with_parent_message_id("msg-root-request"),
            )
            .expect("branch record persists");
    }

    store
        .persist_dead_letter(
            EvidenceDeadLetterInput::new(
                trace_envelope(
                    "agent.local/router",
                    "mesh.task.failed",
                    "corr-span-tree",
                    1_700_000_002_121,
                    "6666666666666666",
                ),
                "msg-dlq-branch",
                "trace-span-tree",
                "dead_lettered",
                DeadLetterFailureCategory::RetryExhausted,
                "retry exhausted with token=must-not-leak",
            )
            .expect("valid branch dead letter input")
            .with_intended_target("agent.local/client")
            .with_attempt_count(3)
            .with_last_failure_category(DeadLetterFailureCategory::Timeout)
            .with_timing(1_700_000_002_121, 1_700_000_002_122, 1_700_000_002_123),
        )
        .expect("branch dead letter persists");
}

fn seed_span_tree_invalid_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("invalid span evidence store opens");
    for (message_id, subject, timestamp, span_id, parent) in [
        (
            "msg-valid-root",
            "mesh.request.created",
            1_700_000_003_000,
            "aaaaaaaaaaaaaaa0",
            None,
        ),
        (
            "msg-valid-child",
            "mesh.work.created",
            1_700_000_003_010,
            "aaaaaaaaaaaaaaa1",
            Some("msg-valid-root"),
        ),
        (
            "msg-self-parent",
            "mesh.work.created",
            1_700_000_003_020,
            "aaaaaaaaaaaaaaa2",
            Some("msg-self-parent"),
        ),
        (
            "msg-cycle-a",
            "mesh.work.created",
            1_700_000_003_030,
            "aaaaaaaaaaaaaaa3",
            Some("msg-cycle-b"),
        ),
        (
            "msg-cycle-b",
            "mesh.work.created",
            1_700_000_003_040,
            "aaaaaaaaaaaaaaa4",
            Some("msg-cycle-a"),
        ),
        (
            "msg-duplicate-child-a",
            "mesh.work.created",
            1_700_000_003_050,
            "aaaaaaaaaaaaaaa5",
            Some("msg-valid-root"),
        ),
        (
            "msg-duplicate-child-b",
            "mesh.work.created",
            1_700_000_003_060,
            "aaaaaaaaaaaaaaa5",
            Some("msg-valid-root"),
        ),
        (
            "msg-missing-parent-child",
            "mesh.work.created",
            1_700_000_003_070,
            "aaaaaaaaaaaaaaa7",
            Some("msg-retained-away"),
        ),
    ] {
        let mut input = EvidenceEnvelopeInput::new(
            trace_envelope(
                "agent.local/diagnostic",
                subject,
                "corr-invalid-span-tree",
                timestamp,
                span_id,
            ),
            message_id,
            "trace-invalid-span-tree",
            "accepted",
        )
        .expect("valid invalid-scenario input")
        .with_target("agent.local/target");
        if let Some(parent) = parent {
            input = input.with_parent_message_id(parent);
        }
        store
            .persist_accepted_envelope(input)
            .expect("invalid-scenario record persists");
    }
}

#[test]
fn read_json_outputs_are_parseable_and_share_top_level_shape() {
    let cases = [
        (vec!["--help", "--output", "json"], "help"),
        (vec!["--version", "--output", "json"], "version"),
        (
            vec![
                "daemon",
                "status",
                "--socket",
                TEST_SOCKET,
                "--output",
                "json",
            ],
            "daemon status",
        ),
        (vec!["daemon", "--help", "--output", "json"], "daemon help"),
        (vec!["agents", "--output", "json"], "agents"),
        (vec!["agents", "--help", "--output", "json"], "agents help"),
        (vec!["doctor", "--output", "json"], "doctor"),
        (vec!["doctor", "--help", "--output", "json"], "doctor help"),
        (vec!["inspect", "--output", "json"], "inspect"),
        (
            vec!["inspect", "messages", "--output", "json"],
            "inspect messages",
        ),
        (vec!["trace", "--help", "--output", "json"], "trace help"),
    ];

    for (args, command) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        let text = stdout(&output);
        assert_no_ansi(&text);
        assert!(!text.contains("zornmesh doctor\n"));
        assert_read_json_contract(&read_json(&output), command);
    }
}

#[test]
fn inspect_messages_json_filters_redacts_and_paginates() {
    let path = temp_evidence_path("messages");
    seed_inspect_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");
    let output = zornmesh(&[
        "inspect",
        "messages",
        "--evidence",
        path,
        "--correlation-id",
        "corr-inspect",
        "--trace-id",
        "trace-inspect",
        "--agent-id",
        "agent.local/source",
        "--subject",
        "mesh.inspect.created",
        "--since",
        "1700000000000",
        "--until",
        "1700000000100",
        "--limit",
        "1",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let text = stdout(&output);
    assert_no_ansi(&text);
    assert!(!text.contains("must-not-leak"));
    let value = read_json(&output);
    assert_read_json_contract(&value, "inspect messages");
    let data = &value["data"];
    assert_eq!(data["collection"], "messages");
    assert_eq!(data["availability"], "available");
    assert_eq!(data["state"], "partial");
    assert_eq!(data["filters"][0]["key"], "correlation_id");
    assert_eq!(data["filters"][0]["value"], "corr-inspect");
    assert_eq!(data["filters"][4]["key"], "since");
    assert_eq!(data["records"].as_array().expect("records array").len(), 1);
    assert_eq!(data["records"][0]["message_id"], "msg-inspect-1");
    assert_eq!(data["records"][0]["delivery_state"], "dead_lettered");
    assert_eq!(data["records"][0]["payload_content_type"], "[REDACTED]");
    assert_eq!(data["pagination"]["limit"], 1);
    assert_eq!(data["pagination"]["total"], 2);
    assert_eq!(data["pagination"]["returned"], 1);
    assert_eq!(data["pagination"]["complete"], false);
    assert_eq!(data["pagination"]["next_cursor"], "1");
    assert_eq!(
        data["metadata"]["evidence_store"]["schema_version"],
        zornmesh_store::EVIDENCE_STORE_SCHEMA_VERSION
    );
    assert_eq!(
        data["metadata"]["runtime"]["status"],
        "unsupported_placeholder"
    );
    assert_eq!(data["metadata"]["release_integrity"]["sbom"], "unavailable");

    let second_page = zornmesh(&[
        "inspect",
        "messages",
        "--evidence",
        path,
        "--correlation-id",
        "corr-inspect",
        "--trace-id",
        "trace-inspect",
        "--agent-id",
        "agent.local/source",
        "--subject",
        "mesh.inspect.created",
        "--since",
        "1700000000000",
        "--until",
        "1700000000100",
        "--limit",
        "1",
        "--cursor",
        "1",
        "--output",
        "json",
    ]);
    assert_success(&second_page);
    let second_page = read_json(&second_page);
    assert_eq!(
        second_page["data"]["records"][0]["message_id"],
        "msg-inspect-2"
    );
    assert_eq!(second_page["data"]["pagination"]["complete"], true);
    assert!(second_page["data"]["pagination"]["next_cursor"].is_null());
}

#[test]
fn inspect_dead_letters_and_audit_apply_structured_filters() {
    let path = temp_evidence_path("dlq-audit");
    seed_inspect_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let dead_letters = zornmesh(&[
        "inspect",
        "dead-letters",
        "--evidence",
        path,
        "--failure-category",
        "retry_exhausted",
        "--agent-id",
        "agent.local/target",
        "--output",
        "json",
    ]);
    assert_success(&dead_letters);
    let dlq = read_json(&dead_letters);
    assert_read_json_contract(&dlq, "inspect dead-letters");
    assert_eq!(dlq["data"]["records"][0]["message_id"], "msg-inspect-1");
    assert_eq!(
        dlq["data"]["records"][0]["failure_category"],
        "retry_exhausted"
    );
    assert_eq!(dlq["data"]["records"][0]["safe_details"], "[REDACTED]");
    assert!(!stdout(&dead_letters).contains("must-not-leak"));

    let audit = zornmesh(&[
        "inspect",
        "audit",
        "--evidence",
        path,
        "--correlation-id",
        "corr-inspect",
        "--trace-id",
        "trace-inspect",
        "--agent-id",
        "agent.local/source",
        "--subject",
        "mesh.inspect.created",
        "--delivery-state",
        "dead_lettered",
        "--output",
        "json",
    ]);
    assert_success(&audit);
    let audit = read_json(&audit);
    assert_read_json_contract(&audit, "inspect audit");
    assert_eq!(audit["data"]["records"].as_array().unwrap().len(), 1);
    assert_eq!(audit["data"]["records"][0]["action"], "dead_lettered");
    assert_eq!(audit["data"]["records"][0]["state_to"], "dead_lettered");
}

#[test]
fn inspect_empty_and_unavailable_states_are_explicit() {
    let empty_path = temp_evidence_path("empty");
    FileEvidenceStore::open_evidence(&empty_path).expect("empty evidence store opens");
    let empty_path = empty_path.to_str().expect("evidence path is utf8");
    let empty = zornmesh(&[
        "inspect",
        "messages",
        "--evidence",
        empty_path,
        "--correlation-id",
        "missing-correlation",
        "--output",
        "json",
    ]);
    assert_success(&empty);
    let empty = read_json(&empty);
    assert_eq!(empty["data"]["availability"], "available");
    assert_eq!(empty["data"]["state"], "empty");
    assert_eq!(empty["data"]["records"].as_array().unwrap().len(), 0);
    assert!(empty["data"]["next_actions"].as_array().unwrap().len() >= 4);
    assert_eq!(empty["data"]["pagination"]["complete"], true);

    let unavailable = zornmesh(&["inspect", "messages", "--output", "json"]);
    assert_success(&unavailable);
    let unavailable = read_json(&unavailable);
    assert_eq!(unavailable["data"]["availability"], "unavailable");
    assert_eq!(unavailable["data"]["state"], "unavailable");
    assert_eq!(unavailable["data"]["records"].as_array().unwrap().len(), 0);
    assert_eq!(
        unavailable["warnings"][0]["code"],
        "W_EVIDENCE_STORE_UNAVAILABLE"
    );

    let missing_path = temp_evidence_path("missing");
    let missing_path_text = missing_path.to_str().expect("evidence path is utf8");
    let missing = zornmesh(&[
        "inspect",
        "messages",
        "--evidence",
        missing_path_text,
        "--output",
        "json",
    ]);
    assert_success(&missing);
    let missing = read_json(&missing);
    assert_eq!(missing["data"]["availability"], "unavailable");
    assert_eq!(
        missing["warnings"][0]["code"],
        "W_EVIDENCE_STORE_UNAVAILABLE"
    );
    assert!(
        !missing_path.exists(),
        "inspect must not create a missing evidence store"
    );
}

#[test]
fn inspect_human_output_is_stable_and_plain() {
    let path = temp_evidence_path("human");
    seed_inspect_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");
    let output = zornmesh(&[
        "inspect",
        "messages",
        "--evidence",
        path,
        "--correlation-id",
        "missing-correlation",
    ]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "zornmesh inspect messages\nstatus: available\nstate: empty\nrecords: 0\nfilters: correlation_id=missing-correlation\nempty: no messages matched the inspect filters\nnext_actions: trace, tail, doctor, retention checks\npagination: complete\n"
    );
    assert_no_ansi(&stdout(&output));
}

#[test]
fn inspect_over_limit_request_returns_stable_validation_error() {
    let output = zornmesh(&["inspect", "messages", "--limit", "101"]);

    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr(&output),
        "E_VALIDATION_FAILED: inspect limit 101 exceeds maximum 100\n"
    );
}

#[test]
fn trace_json_reconstructs_ordered_timeline_and_span_tree() {
    let path = temp_evidence_path("trace-complete");
    seed_trace_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "trace",
        "corr-trace",
        "--evidence",
        path,
        "--span-tree",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let text = stdout(&output);
    assert_no_ansi(&text);
    assert!(!text.contains("must-not-leak"));
    let value = read_json(&output);
    assert_read_json_contract(&value, "trace");
    assert_eq!(value["data"]["correlation_id"], "corr-trace");
    assert_eq!(value["data"]["availability"], "available");
    assert_eq!(value["data"]["state"], "complete");
    assert_eq!(value["data"]["ordering"], "daemon_sequence");

    let timeline = value["data"]["timeline"]
        .as_array()
        .expect("timeline is an array");
    assert!(
        timeline.len() >= 18,
        "timeline includes envelopes, audit entries, and terminal records"
    );
    assert_eq!(timeline[0]["kind"], "envelope");
    assert_eq!(timeline[0]["message_id"], "msg-request");
    assert_eq!(timeline[0]["timestamp_unix_ms"], 1_700_000_000_100_u64);

    let envelope_position = |message_id: &str| {
        timeline
            .iter()
            .position(|event| event["kind"] == "envelope" && event["message_id"] == message_id)
            .expect("message appears in timeline")
    };
    assert!(
        envelope_position("msg-late-reply") > envelope_position("msg-cancel"),
        "daemon sequence, not client timestamp, controls ordering"
    );

    let participants = value["data"]["participants"]
        .as_array()
        .expect("participants array")
        .iter()
        .map(|value| value.as_str().expect("participant string"))
        .collect::<Vec<_>>();
    assert_eq!(
        participants,
        vec!["agent.local/client", "agent.local/worker"]
    );

    let states = value["data"]["delivery_states"]
        .as_array()
        .expect("delivery states array")
        .iter()
        .map(|value| value.as_str().expect("state string"))
        .collect::<Vec<_>>();
    for state in [
        "acknowledged",
        "cancelled",
        "dead_lettered",
        "late_arrival",
        "replayed",
        "retrying",
    ] {
        assert!(states.contains(&state), "missing state {state}");
    }

    let exceptional_states = timeline
        .iter()
        .filter(|event| event["exceptional"] == true)
        .map(|event| {
            event["exceptional_state"]
                .as_str()
                .expect("exceptional state")
        })
        .collect::<Vec<_>>();
    for state in [
        "retry",
        "replay",
        "cancellation",
        "late_arrival",
        "dead_letter",
    ] {
        assert!(
            exceptional_states.contains(&state),
            "missing exceptional state {state}"
        );
    }

    let dead_letter = timeline
        .iter()
        .find(|event| event["kind"] == "dead_letter")
        .expect("dead-letter event appears");
    assert_eq!(dead_letter["message_id"], "msg-dlq");
    assert_eq!(dead_letter["failure_category"], "retry_exhausted");
    assert_eq!(dead_letter["attempt_count"], 3);
    assert_eq!(
        dead_letter["safe_payload_summary"]["payload_content_type"],
        "[REDACTED]"
    );

    let span_tree = &value["data"]["span_tree"];
    assert_eq!(span_tree["reconstruction"], "complete");
    let nodes = span_tree["nodes"].as_array().expect("span nodes array");
    let node = |message_id: &str| {
        nodes
            .iter()
            .find(|node| node["message_id"] == message_id)
            .expect("span node exists")
    };
    assert_eq!(node("msg-request")["span_id"], "1111111111111111");
    assert_eq!(
        node("msg-request")["parent_message_id"],
        serde_json::Value::Null
    );
    assert_eq!(node("msg-request")["relationship"], "root");
    assert_eq!(node("msg-request")["status"], "valid");
    assert_eq!(node("msg-retry")["relationship"], "retry-of");
    assert_eq!(node("msg-replay")["relationship"], "replayed-from");
    assert_eq!(node("msg-late-reply")["relationship"], "responds-to");
    assert_eq!(node("msg-dlq")["relationship"], "dead-letter-terminal");

    let human = zornmesh(&["trace", "corr-trace", "--evidence", path, "--span-tree"]);
    assert_success(&human);
    let human = stdout(&human);
    assert_no_ansi(&human);
    assert!(!human.contains("must-not-leak"));
    assert!(human.contains("state: complete\n"));
    assert!(human.contains(
        "event: sequence=2 kind=envelope message_id=msg-retry state=retrying exceptional=retry relationship=retry-of"
    ));
    assert!(human.contains(
        "event: sequence=6 kind=dead_letter message_id=msg-dlq state=dead_lettered exceptional=dead_letter relationship=dead-letter-terminal"
    ));
    assert!(human.contains(
        "span: message_id=msg-replay span_id=3333333333333333 parent=msg-request relationship=replayed-from status=valid"
    ));
}

#[test]
fn trace_missing_correlation_returns_stable_not_found_json() {
    let path = temp_evidence_path("trace-empty");
    FileEvidenceStore::open_evidence(&path).expect("empty evidence store opens");
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "trace",
        "missing-correlation",
        "--evidence",
        path,
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "trace");
    assert_eq!(value["data"]["correlation_id"], "missing-correlation");
    assert_eq!(value["data"]["availability"], "available");
    assert_eq!(value["data"]["state"], "not_found");
    assert_eq!(value["data"]["timeline"].as_array().unwrap().len(), 0);
    assert_eq!(value["warnings"][0]["code"], "W_TRACE_NOT_FOUND");
    let next_actions = value["data"]["next_actions"]
        .as_array()
        .expect("next actions array")
        .iter()
        .map(|value| value.as_str().expect("next action string"))
        .collect::<Vec<_>>();
    assert_eq!(
        next_actions,
        vec![
            "inspect",
            "doctor",
            "retention checks",
            "audit verification"
        ]
    );
}

#[test]
fn trace_partial_lineage_gap_is_explicit_in_human_and_json() {
    let path = temp_evidence_path("trace-partial");
    seed_partial_trace_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "trace",
        "corr-partial",
        "--evidence",
        path,
        "--span-tree",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "trace");
    assert_eq!(value["data"]["state"], "partial");
    assert_eq!(value["warnings"][0]["code"], "W_TRACE_GAP_DETECTED");
    assert_eq!(value["data"]["gaps"][0]["code"], "missing_parent");
    assert_eq!(value["data"]["gaps"][0]["message_id"], "msg-orphan");
    assert_eq!(
        value["data"]["gaps"][0]["missing_parent_message_id"],
        "msg-missing-parent"
    );
    assert_eq!(value["data"]["span_tree"]["reconstruction"], "partial");
    assert_eq!(value["data"]["span_tree"]["nodes"][0]["status"], "partial");
    assert_eq!(
        value["data"]["span_tree"]["nodes"][0]["invalid_reasons"][0],
        "missing_parent"
    );

    let human = zornmesh(&["trace", "corr-partial", "--evidence", path, "--span-tree"]);
    assert_success(&human);
    let human = stdout(&human);
    assert_no_ansi(&human);
    assert!(human.contains("state: partial\n"));
    assert!(human.contains(
        "gap: missing_parent message_id=msg-orphan missing_parent_message_id=msg-missing-parent"
    ));
    assert!(human.contains("next_actions: inspect, doctor, retention checks, audit verification"));
}

#[test]
fn trace_span_tree_groups_stream_chunks_and_branch_relationships() {
    let path = temp_evidence_path("span-tree-streaming");
    seed_span_tree_streaming_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "trace",
        "corr-span-tree",
        "--evidence",
        path,
        "--span-tree",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let text = stdout(&output);
    assert_no_ansi(&text);
    assert!(!text.contains("must-not-leak"));
    let value = read_json(&output);
    assert_read_json_contract(&value, "trace");
    assert_eq!(value["data"]["state"], "complete");

    let nodes = value["data"]["span_tree"]["nodes"]
        .as_array()
        .expect("span nodes array");
    let node = |message_id: &str| {
        nodes
            .iter()
            .find(|node| node["message_id"] == message_id)
            .expect("span node exists")
    };

    let node_order = nodes
        .iter()
        .map(|node| node["message_id"].as_str().expect("message id"))
        .collect::<Vec<_>>();
    assert_eq!(
        node_order[..7],
        [
            "msg-root-request",
            "msg-stream-001",
            "msg-stream-002",
            "msg-stream-003",
            "msg-stream-cancelled",
            "msg-stream-failed",
            "msg-stream-gap",
        ],
        "stream chunks stay grouped under the request in daemon-sequence order"
    );
    assert_eq!(
        node("msg-root-request")["child_message_ids"],
        serde_json::json!([
            "msg-stream-001",
            "msg-stream-002",
            "msg-stream-003",
            "msg-stream-cancelled",
            "msg-stream-failed",
            "msg-stream-gap",
            "msg-fanout-alpha",
            "msg-fanout-beta",
            "msg-retry-branch",
            "msg-replay-branch",
            "msg-reply",
            "msg-dlq-branch"
        ])
    );
    assert_eq!(node("msg-root-request")["depth"], 0);
    assert_eq!(node("msg-stream-001")["depth"], 1);
    assert_eq!(node("msg-stream-001")["stream_sequence"], 1);
    assert_eq!(node("msg-stream-002")["stream_sequence"], 2);
    assert_eq!(node("msg-stream-003")["stream_sequence"], 3);
    assert_eq!(node("msg-stream-001")["stream_state"], "continue");
    assert_eq!(node("msg-stream-003")["stream_state"], "final");
    assert_eq!(node("msg-stream-cancelled")["stream_state"], "cancelled");
    assert_eq!(node("msg-stream-failed")["stream_state"], "failed");
    assert_eq!(node("msg-stream-gap")["stream_state"], "gap");
    assert_eq!(node("msg-fanout-alpha")["relationship"], "caused-by");
    assert_eq!(node("msg-fanout-beta")["relationship"], "caused-by");
    assert_eq!(node("msg-retry-branch")["relationship"], "retry-of");
    assert_eq!(node("msg-replay-branch")["relationship"], "replayed-from");
    assert_eq!(node("msg-reply")["relationship"], "responds-to");
    assert_eq!(
        node("msg-dlq-branch")["relationship"],
        "dead-letter-terminal"
    );

    let human = zornmesh(&["trace", "corr-span-tree", "--evidence", path, "--span-tree"]);
    assert_success(&human);
    let human = stdout(&human);
    assert_no_ansi(&human);
    assert!(human.contains(
        "span: message_id=msg-stream-003 span_id=dddddddddddddddd parent=msg-root-request relationship=caused-by status=valid depth=1 children=none stream_sequence=3 stream_state=final"
    ));
}

#[test]
fn trace_span_tree_marks_invalid_edges_without_losing_valid_branches() {
    let path = temp_evidence_path("span-tree-invalid");
    seed_span_tree_invalid_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "trace",
        "corr-invalid-span-tree",
        "--evidence",
        path,
        "--span-tree",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "trace");
    assert_eq!(value["data"]["state"], "partial");
    assert_eq!(value["data"]["span_tree"]["reconstruction"], "partial");

    let nodes = value["data"]["span_tree"]["nodes"]
        .as_array()
        .expect("span nodes array");
    let node = |message_id: &str| {
        nodes
            .iter()
            .find(|node| node["message_id"] == message_id)
            .expect("span node exists")
    };
    assert_eq!(node("msg-valid-root")["status"], "valid");
    assert_eq!(node("msg-valid-child")["status"], "valid");
    assert_eq!(
        node("msg-valid-root")["child_message_ids"][0],
        "msg-valid-child"
    );
    assert_eq!(
        node("msg-self-parent")["invalid_reasons"],
        serde_json::json!(["self_parent", "cycle"])
    );
    assert_eq!(
        node("msg-cycle-a")["invalid_reasons"],
        serde_json::json!(["cycle"])
    );
    assert_eq!(
        node("msg-cycle-b")["invalid_reasons"],
        serde_json::json!(["cycle"])
    );
    assert_eq!(
        node("msg-duplicate-child-a")["invalid_reasons"][0],
        "duplicate_edge"
    );
    assert_eq!(
        node("msg-duplicate-child-b")["invalid_reasons"][0],
        "duplicate_edge"
    );
    assert_eq!(
        node("msg-missing-parent-child")["invalid_reasons"],
        serde_json::json!(["missing_parent"])
    );
    assert_eq!(
        node("msg-missing-parent-child")["depth"],
        serde_json::Value::Null
    );

    let human = zornmesh(&[
        "trace",
        "corr-invalid-span-tree",
        "--evidence",
        path,
        "--span-tree",
    ]);
    assert_success(&human);
    let human = stdout(&human);
    assert_no_ansi(&human);
    assert!(human.contains("span_tree: partial"));
    assert!(human.contains("invalid_reasons=duplicate_edge"));
    assert!(human.contains("invalid_reasons=missing_parent"));
}

#[test]
fn streaming_json_outputs_are_parseable_ndjson_events() {
    for format in ["json", "ndjson"] {
        let output = zornmesh(&["trace", "events", "--output", format]);

        assert_success(&output);
        for (index, line) in stdout(&output).lines().enumerate() {
            let event: serde_json::Value =
                serde_json::from_str(line).expect("NDJSON line is valid JSON");
            let object = event.as_object().expect("event is a JSON object");
            assert_eq!(object.len(), 4);
            for key in ["schema_version", "event_type", "sequence", "data"] {
                assert!(object.contains_key(key), "missing event key {key}");
            }
            assert_eq!(event["schema_version"], "zornmesh.cli.event.v1");
            assert_eq!(event["event_type"], "trace.scaffolded");
            assert_eq!(event["sequence"], (index + 1) as u64);
            assert!(event["data"].is_object(), "event data must be an object");
        }
    }
}

#[test]
fn read_commands_emit_stable_human_stdout() {
    let cases = [
        (
            vec!["daemon", "status", "--socket", TEST_SOCKET],
            format!(
                "zornmesh daemon status\nstate: unreachable\nsocket: {TEST_SOCKET}\nsocket_source: cli\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
            ),
        ),
        (
            vec!["agents", "--socket", TEST_SOCKET],
            "zornmesh agents\nstatus: unavailable\nagents: 0\nwarning: agent registry is not available in this scaffold\nremediation: connect agents after identity registration is enabled\n"
                .to_string(),
        ),
        (
            vec!["doctor", "--socket", TEST_SOCKET],
            format!(
                "zornmesh doctor\nstatus: degraded\ndaemon: unreachable\nversion: 0.1.0\nsocket: {TEST_SOCKET}\nsocket_source: cli\nsocket_ownership: unavailable\nsocket_permissions: unavailable\nschema: available (zornmesh.cli.doctor.v1)\notel: unavailable\nsignature: unverifiable\nsbom: unavailable\ntrust: degraded\nshutdown: unavailable\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
            ),
        ),
    ];

    for (args, expected) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        assert_eq!(stdout(&output), expected);
        assert_no_ansi(&stdout(&output));
    }
}

#[test]
fn json_read_commands_emit_only_stable_json_stdout() {
    let cases = [
        (
            vec!["daemon", "status", "--socket", TEST_SOCKET, "--output", "json"],
            format!(
                "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"daemon status\",\"status\":\"ok\",\"data\":{{\"daemon_state\":\"unreachable\",\"socket_path\":\"{TEST_SOCKET}\",\"socket_source\":\"cli\"}},\"warnings\":[]}}\n"
            ),
        ),
        (
            vec!["agents", "--socket", TEST_SOCKET, "--output", "json"],
            "{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"agents\",\"status\":\"ok\",\"data\":{\"registry_status\":\"unavailable\",\"agents\":[]},\"warnings\":[{\"code\":\"W_AGENT_REGISTRY_UNAVAILABLE\",\"message\":\"agent registry is not available in this scaffold\"}]}\n"
                .to_string(),
        ),
        (
            vec!["doctor", "--socket", TEST_SOCKET, "--output", "json"],
            format!(
                "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"doctor\",\"status\":\"ok\",\"data\":{{\"health\":\"degraded\",\"diagnostics_schema\":\"zornmesh.cli.doctor.v1\",\"daemon\":{{\"status\":\"unreachable\",\"version\":\"0.1.0\",\"socket_path\":\"{TEST_SOCKET}\",\"socket_source\":\"cli\",\"remediation\":\"start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\"}},\"socket\":{{\"ownership\":\"unavailable\",\"permissions\":\"unavailable\"}},\"schema\":{{\"status\":\"available\",\"version\":\"zornmesh.cli.doctor.v1\"}},\"otel\":{{\"status\":\"unavailable\",\"endpoint\":\"unconfigured\"}},\"signature\":{{\"status\":\"unverifiable\",\"identity\":\"unavailable\"}},\"sbom\":{{\"status\":\"unavailable\",\"identity\":\"unavailable\"}},\"trust\":{{\"status\":\"degraded\",\"posture\":\"daemon-unreachable\"}},\"shutdown\":{{\"status\":\"unavailable\",\"in_flight_work\":\"unavailable\"}}}},\"warnings\":[{{\"code\":\"W_DAEMON_UNREACHABLE\",\"message\":\"daemon is unreachable; start the daemon or choose another socket\"}},{{\"code\":\"W_OTEL_UNAVAILABLE\",\"message\":\"OTel reachability evidence is not configured for this build\"}},{{\"code\":\"W_SIGNATURE_UNVERIFIABLE\",\"message\":\"build signature evidence is unavailable for this build\"}},{{\"code\":\"W_SBOM_UNAVAILABLE\",\"message\":\"SBOM identity evidence is unavailable for this build\"}}]}}\n"
            ),
        ),
    ];

    for (args, expected) in cases {
        let output = zornmesh(&args);

        assert_success(&output);
        let text = stdout(&output);
        assert_eq!(text, expected);
        assert!(text.starts_with('{'));
        assert!(text.ends_with("}\n"));
        assert_no_ansi(&text);
        assert!(!text.contains("zornmesh doctor\n"));
        assert!(!text.contains("zornmesh agents\n"));
        assert!(!text.contains("zornmesh daemon status\n"));
    }
}

#[test]
fn streaming_json_mode_emits_ndjson_events() {
    let output = zornmesh(&["trace", "events", "--output", "json"]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        "{\"schema_version\":\"zornmesh.cli.event.v1\",\"event_type\":\"trace.scaffolded\",\"sequence\":1,\"data\":{\"status\":\"no_events\"}}\n"
    );
    for line in stdout(&output).lines() {
        assert!(line.starts_with('{'));
        assert!(line.ends_with('}'));
        assert!(line.contains("\"schema_version\":\"zornmesh.cli.event.v1\""));
        assert!(line.contains("\"event_type\":"));
        assert!(line.contains("\"sequence\":"));
        assert!(line.contains("\"data\":"));
    }
}

#[test]
fn doctor_healthy_json_reports_required_diagnostic_categories() {
    let (_listener, path) = healthy_socket("doctor-healthy");
    let socket = path.to_str().expect("socket path is utf8");
    let output = zornmesh(&["doctor", "--socket", socket, "--output", "json"]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "doctor");
    let data = &value["data"];
    assert_eq!(data["health"], "degraded");
    assert_eq!(data["diagnostics_schema"], "zornmesh.cli.doctor.v1");
    assert_eq!(data["daemon"]["status"], "ready");
    assert_eq!(data["daemon"]["version"], "0.1.0");
    assert_eq!(data["daemon"]["socket_path"], socket);
    assert_eq!(data["socket"]["ownership"], "current-user");
    assert_eq!(data["socket"]["permissions"], "private");
    assert_eq!(data["schema"]["status"], "available");
    assert_eq!(data["otel"]["status"], "unavailable");
    assert_eq!(data["signature"]["status"], "unverifiable");
    assert_eq!(data["sbom"]["status"], "unavailable");
    assert_eq!(data["trust"]["status"], "trusted");
    assert_eq!(data["shutdown"]["status"], "idle");
    assert_eq!(
        value["warnings"].as_array().expect("warnings array").len(),
        3
    );
}

#[test]
fn doctor_unsafe_socket_reports_blocked_trust_status() {
    let (_listener, path) = healthy_socket("doctor-unsafe");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("socket made unsafe");
    let socket = path.to_str().expect("socket path is utf8");
    let output = zornmesh(&["doctor", "--socket", socket, "--output", "json"]);

    assert_success(&output);
    let value = read_json(&output);
    let data = &value["data"];
    assert_eq!(data["daemon"]["status"], "blocked");
    assert_eq!(data["socket"]["ownership"], "current-user");
    assert_eq!(data["socket"]["permissions"], "unsafe");
    assert_eq!(data["trust"]["status"], "unsafe");
    assert_eq!(data["trust"]["posture"], "unsafe-socket");
    assert_eq!(value["warnings"][0]["code"], "W_LOCAL_TRUST_UNSAFE");
}

#[test]
fn daemon_shutdown_non_interactive_reports_unreachable_status() {
    let output = zornmesh(&[
        "daemon",
        "shutdown",
        "--socket",
        TEST_SOCKET,
        "--non-interactive",
    ]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "zornmesh daemon shutdown\nstate: unreachable\noutcome: not-running\nsocket: {TEST_SOCKET}\nshutdown_budget_ms: 10000\nin_flight_work: unavailable\nremediation: start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\n"
        )
    );
    assert_no_ansi(&stdout(&output));
}

#[test]
fn daemon_shutdown_json_reports_stable_outcome() {
    let output = zornmesh(&[
        "daemon",
        "shutdown",
        "--socket",
        TEST_SOCKET,
        "--non-interactive",
        "--output",
        "json",
    ]);

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "{{\"schema_version\":\"zornmesh.cli.read.v1\",\"command\":\"daemon shutdown\",\"status\":\"ok\",\"data\":{{\"daemon_state\":\"unreachable\",\"outcome\":\"not-running\",\"socket_path\":\"{TEST_SOCKET}\",\"shutdown_budget_ms\":10000,\"in_flight_work\":\"unavailable\",\"remediation\":\"start the daemon with `zornmesh daemon --socket {TEST_SOCKET}`\"}},\"warnings\":[]}}\n"
        )
    );
}

#[test]
fn supported_shell_completion_contains_initial_commands_and_flags() {
    for shell in ["bash", "zsh", "fish"] {
        let output = zornmesh(&["completion", shell]);

        assert_success(&output);
        let text = stdout(&output);
        assert_no_ansi(&text);
        assert!(text.contains("zornmesh"), "completion names binary");
        for token in [
            "daemon",
            "doctor",
            "agents",
            "stdio",
            "help",
            "--output",
            "--socket",
            "--non-interactive",
        ] {
            assert!(
                text.contains(token),
                "{shell} completion must include {token}"
            );
        }
    }
}

#[test]
fn stdio_daemon_unavailable_reports_protocol_error_to_host() {
    let output = zornmesh(&[
        "stdio",
        "--as-agent",
        "agent.local/mcp-host",
        "--socket",
        TEST_SOCKET,
    ]);

    assert!(
        output.status.success(),
        "protocol errors are reported on stdout for MCP hosts, got stderr {:?}",
        stderr(&output)
    );
    assert!(output.stderr.is_empty());
    let value = read_json(&output);
    assert_eq!(value["jsonrpc"], "2.0");
    assert!(value["id"].is_null());
    assert_eq!(value["error"]["data"]["code"], "E_DAEMON_UNREACHABLE");
    assert!(!stdout(&output).contains(TEST_SOCKET));
}

#[test]
fn stdio_initialize_over_ready_socket_emits_mcp_ack() {
    let (_listener, path) = healthy_socket("stdio-init");
    let socket = path.to_str().expect("socket path is utf8");
    let mut child = zornmesh_command(&[
        "stdio",
        "--as-agent",
        "agent.local/mcp-host",
        "--socket",
        socket,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("zornmesh stdio starts");
    {
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(
                br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            )
            .expect("initialize written");
        stdin.write_all(b"\n").expect("newline written");
    }

    let output = child.wait_with_output().expect("stdio exits after EOF");
    assert_success(&output);
    let value = read_json(&output);
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert_eq!(value["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(value["result"]["serverInfo"]["name"], "zornmesh-stdio");
}

#[test]
fn stdio_unsupported_capability_result_is_structured_and_redacted() {
    let (_listener, path) = healthy_socket("stdio-unsupported");
    let socket = path.to_str().expect("socket path is utf8");
    let mut child = zornmesh_command(&[
        "stdio",
        "--as-agent",
        "agent.local/mcp-host",
        "--socket",
        socket,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("zornmesh stdio starts");
    {
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        stdin
            .write_all(
                br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            )
            .expect("initialize written");
        stdin.write_all(b"\n").expect("newline written");
        stdin
            .write_all(
                br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"capability_id":"stream.tokens","requires_streaming":true,"password":"do-not-leak"}}"#,
            )
            .expect("tool call written");
        stdin.write_all(b"\n").expect("newline written");
    }

    let output = child.wait_with_output().expect("stdio exits after EOF");
    assert_success(&output);
    let stdout = stdout(&output);
    let responses: Vec<_> = stdout.lines().collect();
    assert_eq!(
        responses.len(),
        2,
        "expected initialize and tool-call responses"
    );
    let unsupported: serde_json::Value =
        serde_json::from_str(responses[1]).expect("tool-call response is JSON");
    assert_eq!(unsupported["jsonrpc"], "2.0");
    assert_eq!(unsupported["id"], 2);
    assert_eq!(unsupported["result"]["status"], "unsupported_capability");
    assert_eq!(
        unsupported["result"]["code"],
        "E_BRIDGE_UNSUPPORTED_CAPABILITY"
    );
    assert_eq!(unsupported["result"]["capability_id"], "stream.tokens");
    assert!(
        unsupported["result"]["remediation"]
            .as_str()
            .expect("remediation is a string")
            .contains("zornmesh CLI")
    );
    let serialized = unsupported.to_string();
    assert!(serialized.contains("[REDACTED]"));
    assert!(!serialized.contains("do-not-leak"));
}

#[test]
fn unsupported_shell_completion_fails_without_stdout() {
    let output = zornmesh(&["completion", "powershell"]);

    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr(&output),
        "E_UNSUPPORTED_SHELL: unsupported shell 'powershell'; supported shells: bash, zsh, fish\n"
    );
}

#[test]
fn no_color_keeps_human_plain_and_json_byte_identical() {
    let mut human = zornmesh_command(&["doctor", "--socket", TEST_SOCKET]);
    human.env("NO_COLOR", "1");
    let human = human.output().expect("zornmesh binary runs");
    assert_success(&human);
    assert_no_ansi(&stdout(&human));

    let json = zornmesh(&["doctor", "--socket", TEST_SOCKET, "--output", "json"]);
    let mut json_no_color =
        zornmesh_command(&["doctor", "--socket", TEST_SOCKET, "--output", "json"]);
    json_no_color.env("NO_COLOR", "1");
    let json_no_color = json_no_color.output().expect("zornmesh binary runs");

    assert_success(&json);
    assert_success(&json_no_color);
    assert_eq!(json.stdout, json_no_color.stdout);
}

#[test]
fn exit_contract_maps_registered_error_categories() {
    let cases = [
        (
            vec!["missing"],
            64,
            "E_UNSUPPORTED_COMMAND: unsupported zornmesh command 'missing'\n",
        ),
        (
            vec![
                "daemon",
                "status",
                "--socket",
                TEST_SOCKET,
                "--require-ready",
            ],
            69,
            "E_DAEMON_UNREACHABLE: daemon is unreachable at /tmp/zorn-cli-contract.sock; start the daemon or choose another socket\n",
        ),
        (
            vec!["agents", "inspect", ""],
            65,
            "E_VALIDATION_FAILED: agent id must not be empty\n",
        ),
        (
            vec!["agents", "inspect", "missing-agent"],
            66,
            "E_NOT_FOUND: agent 'missing-agent' was not found\n",
        ),
        (
            vec!["doctor", "--output", "yaml"],
            64,
            "E_UNSUPPORTED_OUTPUT_FORMAT: unsupported output format 'yaml'; supported formats: human, json, ndjson\n",
        ),
    ];

    for (args, code, expected_stderr) in cases {
        let output = zornmesh(&args);

        assert_eq!(output.status.code(), Some(code), "args: {args:?}");
        assert!(
            output.stdout.is_empty(),
            "stdout must stay empty on failure"
        );
        assert_eq!(stderr(&output), expected_stderr);
    }
}

#[test]
fn configuration_precedence_is_deterministic() {
    let config = temp_config("socket_path=/tmp/zorn-config.sock\n");
    let config = config.to_str().expect("temp path is utf8");

    let config_output = zornmesh(&["daemon", "status", "--config", config, "--output", "json"]);
    assert_success(&config_output);
    assert!(stdout(&config_output).contains("\"socket_path\":\"/tmp/zorn-config.sock\""));
    assert!(stdout(&config_output).contains("\"socket_source\":\"config\""));

    let mut env_command =
        zornmesh_command(&["daemon", "status", "--config", config, "--output", "json"]);
    env_command.env("ZORN_SOCKET_PATH", "/tmp/zorn-env.sock");
    let env_output = env_command.output().expect("zornmesh binary runs");
    assert_success(&env_output);
    assert!(stdout(&env_output).contains("\"socket_path\":\"/tmp/zorn-env.sock\""));
    assert!(stdout(&env_output).contains("\"socket_source\":\"env\""));

    let mut cli_command = zornmesh_command(&[
        "daemon",
        "status",
        "--config",
        config,
        "--socket",
        "/tmp/zorn-cli.sock",
        "--output",
        "json",
    ]);
    cli_command.env("ZORN_SOCKET_PATH", "/tmp/zorn-env.sock");
    let cli_output = cli_command.output().expect("zornmesh binary runs");
    assert_success(&cli_output);
    assert!(stdout(&cli_output).contains("\"socket_path\":\"/tmp/zorn-cli.sock\""));
    assert!(stdout(&cli_output).contains("\"socket_source\":\"cli\""));
}

fn seed_tail_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("tail evidence store opens");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.tail.created",
                    "corr-tail-1",
                    1_700_000_000_001,
                ),
                "msg-tail-1",
                "trace-tail-1",
                "accepted",
            )
            .expect("first envelope input")
            .with_target("agent.local/target"),
        )
        .expect("first tail envelope persists");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.tail.completed",
                    "corr-tail-2",
                    1_700_000_000_002,
                ),
                "msg-tail-2",
                "trace-tail-2",
                "accepted",
            )
            .expect("second envelope input")
            .with_target("agent.local/target"),
        )
        .expect("second tail envelope persists");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.other.created",
                    "corr-other",
                    1_700_000_000_003,
                ),
                "msg-other",
                "trace-other",
                "accepted",
            )
            .expect("other envelope input"),
        )
        .expect("non-matching envelope persists");
}

#[test]
fn tail_json_emits_ndjson_events_in_daemon_sequence_order() {
    let path = temp_evidence_path("tail-json");
    seed_tail_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "tail",
        "mesh.tail.*",
        "--evidence",
        path,
        "--output",
        "json",
    ]);

    assert_success(&output);
    let text = stdout(&output);
    assert_no_ansi(&text);
    assert!(!text.contains("must-not-leak"));

    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "status + 2 events + status");

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first line is JSON");
    assert_eq!(first["schema_version"], "zornmesh.cli.event.v1");
    assert_eq!(first["command"], "tail");
    assert_eq!(first["kind"], "status");
    assert_eq!(first["data"]["status"], "backfill");
    assert_eq!(first["data"]["subject_pattern"], "mesh.tail.*");
    assert_eq!(first["data"]["ordering"], "daemon_sequence");

    let event_one: serde_json::Value = serde_json::from_str(lines[1]).expect("event line 1");
    assert_eq!(event_one["kind"], "event");
    assert_eq!(event_one["data"]["message_id"], "msg-tail-1");
    assert_eq!(event_one["data"]["subject"], "mesh.tail.created");
    assert_eq!(event_one["data"]["correlation_id"], "corr-tail-1");
    assert_eq!(event_one["data"]["delivery_state"], "accepted");
    assert_eq!(
        event_one["data"]["safe_payload_summary"]["payload_content_type"],
        "[REDACTED]"
    );

    let event_two: serde_json::Value = serde_json::from_str(lines[2]).expect("event line 2");
    assert_eq!(event_two["data"]["message_id"], "msg-tail-2");
    assert_eq!(event_two["data"]["subject"], "mesh.tail.completed");
    assert!(
        event_one["data"]["daemon_sequence"].as_u64()
            < event_two["data"]["daemon_sequence"].as_u64(),
        "events ordered by daemon sequence"
    );

    let last: serde_json::Value = serde_json::from_str(lines[3]).expect("trailing status JSON");
    assert_eq!(last["kind"], "status");
    assert_eq!(last["data"]["status"], "stale");
    assert_eq!(last["data"]["matched"], 2);
}

#[test]
fn tail_human_emits_redacted_lines_and_skips_non_matching() {
    let path = temp_evidence_path("tail-human");
    seed_tail_evidence(&path);
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&["tail", "mesh.tail.*", "--evidence", path]);

    assert_success(&output);
    let text = stdout(&output);
    assert_no_ansi(&text);
    assert!(!text.contains("must-not-leak"));
    assert!(!text.contains("mesh.other"));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "only matching envelopes appear in tail");
    assert!(lines[0].contains("subject=mesh.tail.created"));
    assert!(lines[0].contains("from=agent.local/source"));
    assert!(lines[0].contains("to=agent.local/target"));
    assert!(lines[0].contains("state=accepted"));
    assert!(lines[0].contains("corr=corr-tail-1"));
    assert!(lines[1].contains("subject=mesh.tail.completed"));
}

#[test]
fn tail_disconnected_when_evidence_store_missing_emits_status_event() {
    let dir = std::env::temp_dir().join(format!(
        "zornmesh-tail-missing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos(),
    ));
    let missing_path = dir.join("evidence.log");
    let path = missing_path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&[
        "tail",
        "mesh.tail.*",
        "--evidence",
        path,
        "--output",
        "ndjson",
    ]);

    assert!(output.status.success(), "tail exits successfully when daemon evidence is missing");
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "disconnected backfill + disconnected status");
    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first JSON line");
    assert_eq!(first["data"]["status"], "disconnected");
    assert!(
        first["data"]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("evidence store does not exist")
    );
    let last: serde_json::Value = serde_json::from_str(lines[1]).expect("last JSON line");
    assert_eq!(last["data"]["status"], "disconnected");
    assert_eq!(last["data"]["matched"], 0);
}

fn seed_replay_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("replay evidence store opens");
    store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(
                evidence_envelope(
                    "agent.local/source",
                    "mesh.replay.original",
                    "corr-replay",
                    1_700_000_000_111,
                ),
                "msg-replay-original",
                "trace-replay",
                "accepted",
            )
            .expect("replay envelope input")
            .with_target("agent.local/target"),
        )
        .expect("replay envelope persists");
}

#[test]
fn replay_preview_emits_eligibility_and_confirmation_token_without_side_effect() {
    let path = temp_evidence_path("replay-preview");
    seed_replay_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();
    let baseline_count = {
        let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
        store.audit_entries().len()
    };

    let output = zornmesh(&[
        "replay",
        "msg-replay-original",
        "--evidence",
        &path_str,
        "--preview",
        "--output",
        "json",
    ]);
    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "replay");
    assert_eq!(value["data"]["mode"], "preview");
    assert_eq!(value["data"]["eligibility"], "eligible");
    assert_eq!(value["data"]["side_effect"], false);
    assert_eq!(value["data"]["original_message_id"], "msg-replay-original");
    assert_eq!(value["data"]["replay_lineage"]["replayed_from"], "msg-replay-original");
    let token = value["data"]["confirmation_token"]
        .as_str()
        .expect("preview emits confirmation token");
    assert!(token.starts_with("zmpv-"));
    assert_eq!(value["data"]["target"], "agent.local/target");
    assert_eq!(value["data"]["subject"], "mesh.replay.original");

    let post_count = {
        let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
        store.audit_entries().len()
    };
    assert_eq!(
        post_count, baseline_count,
        "preview must not persist new audit evidence"
    );
}

#[test]
fn replay_without_confirmation_refuses_with_stable_reason() {
    let path = temp_evidence_path("replay-refuse");
    seed_replay_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();
    let baseline_count = {
        let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
        store.audit_entries().len()
    };

    let output = zornmesh(&[
        "replay",
        "msg-replay-original",
        "--evidence",
        &path_str,
        "--output",
        "json",
    ]);
    assert!(!output.status.success(), "missing confirmation refuses replay");
    assert_eq!(output.status.code(), Some(65));
    assert!(stderr(&output).contains("E_REPLAY_CONFIRMATION_REQUIRED"));

    let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
    assert_eq!(
        store.audit_entries().len(),
        baseline_count,
        "refused replay must not persist new audit evidence"
    );
}

#[test]
fn replay_with_yes_persists_replay_lineage_audit_entry() {
    let path = temp_evidence_path("replay-yes");
    seed_replay_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();
    let baseline_count = {
        let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
        store.audit_entries().len()
    };

    let output = zornmesh(&[
        "replay",
        "msg-replay-original",
        "--evidence",
        &path_str,
        "--yes",
        "--output",
        "json",
    ]);
    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "replay");
    assert_eq!(value["data"]["mode"], "commit");
    assert_eq!(value["data"]["side_effect"], true);
    assert_eq!(value["data"]["replay_lineage"]["replayed_from"], "msg-replay-original");

    let store = FileEvidenceStore::open_evidence(&path).expect("evidence store reopens");
    let audit = store.audit_entries();
    assert_eq!(
        audit.len(),
        baseline_count + 1,
        "commit persists exactly one new audit entry"
    );
    let last = audit.last().expect("trailing audit entry");
    assert_eq!(last.action(), "replay_requested");
    assert_eq!(last.state_to(), "replayed");
    assert_eq!(last.message_id(), "msg-replay-original");
    assert!(last.outcome_details().contains("replayed from msg-replay-original"));
}

#[test]
fn replay_with_stale_confirmation_token_refuses() {
    let path = temp_evidence_path("replay-stale");
    seed_replay_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let output = zornmesh(&[
        "replay",
        "msg-replay-original",
        "--evidence",
        &path_str,
        "--confirmation-token",
        "zmpv-deadbeefdeadbeef",
        "--output",
        "json",
    ]);
    assert!(!output.status.success(), "stale token refuses replay");
    assert_eq!(output.status.code(), Some(65));
    assert!(stderr(&output).contains("E_REPLAY_STALE_CONFIRMATION"));
}

#[test]
fn replay_missing_message_returns_not_found_refusal() {
    let path = temp_evidence_path("replay-missing");
    FileEvidenceStore::open_evidence(&path).expect("empty evidence store opens");
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let output = zornmesh(&[
        "replay",
        "missing-msg",
        "--evidence",
        &path_str,
        "--preview",
        "--output",
        "json",
    ]);
    assert!(!output.status.success(), "missing record cannot be replayed");
    assert_eq!(output.status.code(), Some(66));
    assert!(stderr(&output).contains("E_REPLAY_NOT_FOUND"));
}

fn seed_retention_evidence(path: &std::path::Path) {
    let store = FileEvidenceStore::open_evidence(path).expect("retention evidence store opens");
    for index in 0..3u64 {
        let timestamp = 1_700_000_000_000 + index;
        let message_id = format!("msg-retention-{index}");
        let correlation_id = format!("corr-retention-{index}");
        let trace_id = format!("trace-retention-{index}");
        store
            .persist_accepted_envelope(
                EvidenceEnvelopeInput::new(
                    evidence_envelope(
                        "agent.local/source",
                        "mesh.retention.created",
                        &correlation_id,
                        timestamp,
                    ),
                    &message_id,
                    &trace_id,
                    "accepted",
                )
                .expect("retention envelope input")
                .with_target("agent.local/target"),
            )
            .expect("retention envelope persists");
    }
}

#[test]
fn retention_plan_marks_aged_records_with_retention_checkpoint() {
    let path = temp_evidence_path("retention-aged");
    seed_retention_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let output = zornmesh(&[
        "retention",
        "plan",
        "--evidence",
        &path_str,
        "--max-age-ms",
        "100",
        "--now-unix-ms",
        "1700000001000",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_read_json_contract(&value, "retention");
    assert_eq!(value["data"]["state"], "purge_required");
    assert_eq!(value["data"]["mode"], "plan");
    assert_eq!(value["data"]["policy"]["max_age_ms"], 100);
    assert_eq!(value["data"]["retained_envelope_count"], 0);
    let purgeable_ids = value["data"]["purgeable_envelope_ids"]
        .as_array()
        .expect("purgeable envelope id list");
    assert_eq!(purgeable_ids.len(), 3);

    let checkpoint = &value["data"]["retention_checkpoint"];
    assert!(!checkpoint.is_null(), "retention checkpoint metadata present");
    assert_eq!(checkpoint["sequence_start"], 1);
    assert_eq!(checkpoint["sequence_end"], 3);
    assert_eq!(checkpoint["purge_reason"], "max_age_exceeded");
    assert_eq!(checkpoint["purged_audit_count"], 3);
    assert!(
        checkpoint["prior_audit_hash"]
            .as_str()
            .map(str::is_empty)
            == Some(false),
        "prior audit hash anchors the chain"
    );
    assert_ne!(
        checkpoint["last_audit_hash"], checkpoint["prior_audit_hash"],
        "retention checkpoint preserves first->last hash boundaries"
    );

    let warnings = value["warnings"].as_array().expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"] == "W_RETENTION_GAP_PROJECTED"),
        "retention plan warns about projected retention gap"
    );
}

#[test]
fn retention_plan_with_no_records_reports_no_purge_required() {
    let path = temp_evidence_path("retention-empty");
    FileEvidenceStore::open_evidence(&path).expect("empty evidence store opens");
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let output = zornmesh(&[
        "retention",
        "plan",
        "--evidence",
        &path_str,
        "--max-age-ms",
        "1000",
        "--now-unix-ms",
        "1700000000000",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_eq!(value["data"]["state"], "no_purge_required");
    assert!(
        value["data"]["retention_checkpoint"].is_null(),
        "no records means no checkpoint metadata"
    );
    assert!(
        value["warnings"]
            .as_array()
            .expect("warnings array")
            .is_empty(),
        "no purge means no retention-gap warning"
    );
}

#[test]
fn retention_plan_invalid_config_is_rejected_with_stable_validation_error() {
    let path = temp_evidence_path("retention-invalid");
    FileEvidenceStore::open_evidence(&path).expect("evidence store opens");
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let zero_age = zornmesh(&[
        "retention",
        "plan",
        "--evidence",
        &path_str,
        "--max-age-ms",
        "0",
    ]);
    assert!(!zero_age.status.success());
    assert_eq!(zero_age.status.code(), Some(65));
    assert!(stderr(&zero_age).contains("E_VALIDATION_FAILED"));

    let no_policy = zornmesh(&["retention", "plan", "--evidence", &path_str]);
    assert!(!no_policy.status.success());
    assert_eq!(no_policy.status.code(), Some(65));
    assert!(stderr(&no_policy).contains("E_VALIDATION_FAILED"));
}

#[test]
fn retention_plan_max_count_marks_oldest_envelopes_for_purge() {
    let path = temp_evidence_path("retention-count");
    seed_retention_evidence(&path);
    let path_str = path.to_str().expect("evidence path is utf8").to_owned();

    let output = zornmesh(&[
        "retention",
        "plan",
        "--evidence",
        &path_str,
        "--max-count",
        "1",
        "--now-unix-ms",
        "1700000010000",
        "--output",
        "json",
    ]);

    assert_success(&output);
    let value = read_json(&output);
    assert_eq!(value["data"]["state"], "purge_required");
    assert_eq!(value["data"]["retained_envelope_count"], 1);
    let ids = value["data"]["purgeable_envelope_ids"]
        .as_array()
        .expect("purgeable envelope id list")
        .iter()
        .map(|value| value.as_str().expect("id string"))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["msg-retention-0", "msg-retention-1"]);
    assert_eq!(
        value["data"]["retention_checkpoint"]["purge_reason"],
        "max_count_exceeded"
    );
}

#[test]
fn tail_invalid_subject_pattern_returns_validation_error() {
    let path = temp_evidence_path("tail-invalid");
    FileEvidenceStore::open_evidence(&path).expect("empty evidence store opens");
    let path = path.to_str().expect("evidence path is utf8");

    let output = zornmesh(&["tail", "zorn.reserved", "--evidence", path]);
    assert!(!output.status.success(), "reserved prefix is rejected");
    assert_eq!(output.status.code(), Some(65));
    assert!(stderr(&output).contains("E_SUBJECT_VALIDATION"));
}
