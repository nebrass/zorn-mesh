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
use zornmesh_store::{EvidenceQuery, EvidenceStore, FileEvidenceStore};

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

    let store = FileEvidenceStore::open_evidence(&evidence_path).expect("reopen evidence store");
    let records =
        store.query_envelopes(EvidenceQuery::new().correlation_id("corr-daemon-evidence"));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message_id(), "corr-daemon-evidence");
    assert_eq!(records[0].trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(records[0].delivery_state(), "accepted");
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
