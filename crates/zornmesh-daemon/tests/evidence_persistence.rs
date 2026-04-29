use std::{
    os::unix::net::UnixStream,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zornmesh_core::{CoordinationOutcomeKind, CoordinationStage, Envelope};
use zornmesh_daemon::{DaemonConfig, DaemonErrorCode, DaemonRuntime};
use zornmesh_proto::{
    ClientFrame, FrameStatus, ServerFrame, read_server_frame, write_client_frame,
};
use zornmesh_store::{
    DeadLetterFailureCategory, DeadLetterQuery, EvidenceQuery, EvidenceStore, FileEvidenceStore,
};

fn unique_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after epoch")
        .as_nanos();
    let short_name: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect();
    std::env::temp_dir().join(format!("zme{short_name}-{}-{nanos}", std::process::id()))
}

fn test_config(socket_path: PathBuf, evidence_path: PathBuf) -> DaemonConfig {
    DaemonConfig::for_test(socket_path)
        .allow_elevated_for_tests(true)
        .with_shutdown_budget(Duration::from_millis(1))
        .with_evidence_store_path(evidence_path)
}

#[test]
fn publish_emits_durable_ack_only_after_evidence_commit() {
    let dir = unique_dir("publish");
    let socket_path = dir.join("z");
    let evidence_path = dir.join("evidence.log");
    let daemon = DaemonRuntime::start(test_config(socket_path.clone(), evidence_path.clone()))
        .expect("daemon starts with evidence store");
    let envelope = Envelope::with_trace_context(
        "agent.local/source",
        "mesh.work.created",
        b"{}".to_vec(),
        42,
        "corr-daemon-evidence",
        "application/json",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        Some("rojo=00f067aa0ba902b7"),
    )
    .expect("valid envelope");

    let mut subscription_stream = UnixStream::connect(&socket_path).expect("connect subscriber");
    write_client_frame(
        &mut subscription_stream,
        &ClientFrame::Subscribe {
            pattern: "mesh.work.>".to_owned(),
        },
    )
    .expect("write subscribe frame");
    assert!(daemon.accept_once().expect("accept subscribe connection"));
    match read_server_frame(&mut subscription_stream).expect("subscription accepted") {
        ServerFrame::SendResult(result) => assert_eq!(result.status(), FrameStatus::Accepted),
        other => panic!("unexpected subscription frame {other:?}"),
    }

    let mut stream = UnixStream::connect(&socket_path).expect("connect to daemon");
    write_client_frame(
        &mut stream,
        &ClientFrame::Publish {
            envelope: Box::new(envelope.clone()),
        },
    )
    .expect("write publish frame");
    assert!(daemon.accept_once().expect("accept publish connection"));

    let result = match read_server_frame(&mut stream).expect("server reply") {
        ServerFrame::SendResult(result) => result,
        other => panic!("unexpected server frame {other:?}"),
    };

    assert_eq!(result.status(), FrameStatus::Accepted);
    let durable = result.durable_outcome().expect("durable outcome emitted");
    assert_eq!(durable.kind(), CoordinationOutcomeKind::DurableAccepted);
    assert_eq!(durable.stage(), CoordinationStage::Durable);
    assert_eq!(durable.code(), "DURABLE_ACCEPTED");
    match read_server_frame(&mut subscription_stream).expect("delivery received") {
        ServerFrame::Delivery { envelope, .. } => {
            assert_eq!(envelope.correlation_id(), "corr-daemon-evidence");
        }
        other => panic!("unexpected delivery frame {other:?}"),
    }

    let store = FileEvidenceStore::open_evidence(&evidence_path).expect("reopen evidence store");
    let records =
        store.query_envelopes(EvidenceQuery::new().correlation_id("corr-daemon-evidence"));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message_id(), "corr-daemon-evidence");
    assert_eq!(records[0].trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(records[0].delivery_state(), "accepted");
}

#[test]
fn publish_without_recipient_persists_no_recipient_dead_letter_before_ack() {
    let dir = unique_dir("dlq");
    let socket_path = dir.join("z");
    let evidence_path = dir.join("evidence.log");
    let daemon = DaemonRuntime::start(test_config(socket_path.clone(), evidence_path.clone()))
        .expect("daemon starts with evidence store");
    let envelope = Envelope::with_trace_context(
        "agent.local/source",
        "mesh.work.unrouted",
        b"{\"token\":\"must-not-persist\"}".to_vec(),
        42,
        "corr-daemon-dlq",
        "application/json",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        Some("rojo=00f067aa0ba902b7"),
    )
    .expect("valid envelope");

    let mut stream = UnixStream::connect(&socket_path).expect("connect to daemon");
    write_client_frame(
        &mut stream,
        &ClientFrame::Publish {
            envelope: Box::new(envelope),
        },
    )
    .expect("write publish frame");
    assert!(daemon.accept_once().expect("accept publish connection"));

    let result = match read_server_frame(&mut stream).expect("server reply") {
        ServerFrame::SendResult(result) => result,
        other => panic!("unexpected server frame {other:?}"),
    };

    assert_eq!(result.status(), FrameStatus::Accepted);
    assert_eq!(result.outcome().delivery_attempts(), 0);

    let store = FileEvidenceStore::open_evidence(&evidence_path).expect("reopen evidence store");
    let dead_letters = store.query_dead_letters(
        DeadLetterQuery::new()
            .correlation_id("corr-daemon-dlq")
            .subject("mesh.work.unrouted")
            .agent_id("agent.local/source")
            .failure_category(DeadLetterFailureCategory::NoEligibleRecipient),
    );
    assert_eq!(dead_letters.len(), 1);
    let dead_letter = &dead_letters[0];
    assert_eq!(dead_letter.message_id(), "corr-daemon-dlq");
    assert_eq!(dead_letter.terminal_state(), "dead_lettered");
    assert_eq!(
        dead_letter.safe_details(),
        "no eligible recipient matched subject"
    );
    assert_eq!(dead_letter.attempt_count(), 0);

    let envelopes = store.query_envelopes(EvidenceQuery::new().correlation_id("corr-daemon-dlq"));
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].delivery_state(), "dead_lettered");
    let persisted = std::fs::read_to_string(&evidence_path).expect("evidence file readable");
    assert!(!persisted.contains("must-not-persist"));
}

#[test]
fn corrupt_evidence_store_prevents_daemon_start_and_durable_ack() {
    let dir = unique_dir("corrupt");
    std::fs::create_dir_all(&dir).expect("create test dir");
    let socket_path = dir.join("z");
    let evidence_path = dir.join("evidence.log");
    std::fs::write(&evidence_path, "v1|tx|truncated\n").expect("write corrupt evidence");

    let err = DaemonRuntime::start(test_config(socket_path, evidence_path))
        .expect_err("daemon refuses corrupt evidence store");

    assert_eq!(err.code(), DaemonErrorCode::PersistenceUnavailable);
    assert!(err.to_string().contains("E_PERSISTENCE_UNAVAILABLE"));
}
