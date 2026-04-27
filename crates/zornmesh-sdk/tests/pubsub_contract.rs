use std::{
    fs,
    io::Write,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zornmesh_core::Envelope;
use zornmesh_proto::{FrameStatus, MAX_FRAME_BYTES, ServerFrame, read_server_frame};
use zornmesh_sdk::{ConnectOptions, Mesh, SdkErrorCode, SendStatus};

fn unique_socket(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "zornmesh-sdk-pubsub-{name}-{}-{nanos}",
            std::process::id()
        ))
        .join("zorn.sock")
}

struct AutoSpawnCleanup {
    path: PathBuf,
}

impl AutoSpawnCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for AutoSpawnCleanup {
    fn drop(&mut self) {
        Mesh::shutdown_autospawned_daemon_for_tests(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn autospawn_options(path: PathBuf) -> ConnectOptions {
    ConnectOptions::for_socket(path)
        .allow_elevated_daemon_for_tests()
        .with_connect_timeout(Duration::from_millis(200))
}

#[test]
fn two_rust_clients_publish_and_receive_first_local_envelope() {
    let path = unique_socket("happy-path");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let subscriber = Mesh::connect_with_options(autospawn_options(path.clone()))
        .expect("subscriber connects to local daemon");
    let publisher = Mesh::connect_with_options(autospawn_options(path))
        .expect("publisher connects to same local daemon");
    let mut subscription = subscriber
        .subscribe("mesh.trace.>")
        .expect("subscriber registers prefix pattern");
    let envelope = Envelope::new(
        "agent.local/publisher",
        "mesh.trace.created",
        b"{\"trace_id\":\"trace-1\"}".to_vec(),
    )
    .expect("valid first envelope");
    let correlation_id = envelope.correlation_id().to_owned();
    let timestamp = envelope.timestamp_unix_ms();

    let result = publisher.publish(&envelope);

    assert_eq!(result.status(), SendStatus::Accepted);
    let delivery = subscription
        .recv_delivery(Duration::from_millis(500))
        .expect("subscriber receives one delivery attempt")
        .expect("matching envelope is delivered");

    assert_eq!(delivery.attempt(), 1);
    assert_eq!(delivery.envelope().source_agent(), "agent.local/publisher");
    assert_eq!(delivery.envelope().subject(), "mesh.trace.created");
    assert_eq!(delivery.envelope().correlation_id(), correlation_id);
    assert_eq!(delivery.envelope().timestamp_unix_ms(), timestamp);
    assert_eq!(
        delivery.envelope().payload_metadata().content_type(),
        "application/octet-stream"
    );
    assert_eq!(
        delivery.envelope().payload_metadata().payload_len(),
        envelope.payload().len()
    );
    assert_eq!(delivery.envelope().payload(), envelope.payload());
}

#[test]
fn publish_result_distinguishes_unreachable_daemon() {
    let path = unique_socket("unreachable");
    let publisher = Mesh::for_test_socket(path);
    let envelope = Envelope::new(
        "agent.local/publisher",
        "mesh.trace.created",
        b"{}".to_vec(),
    )
    .expect("valid envelope");

    let result = publisher.publish(&envelope);

    assert_eq!(result.status(), SendStatus::DaemonUnreachable);
    assert_eq!(result.code(), "E_DAEMON_UNREACHABLE");
}

#[test]
fn publish_result_distinguishes_validation_failed_payload_limit() {
    let path = unique_socket("payload-limit");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let publisher = Mesh::connect_with_options(autospawn_options(path))
        .expect("publisher connects to local daemon");
    let envelope = Envelope::with_metadata(
        "agent.local/publisher",
        "mesh.trace.created",
        vec![0; MAX_FRAME_BYTES],
        1,
        "corr-payload-limit",
        "application/octet-stream",
    )
    .expect("envelope payload is within core limit but exceeds framed transport budget");

    let result = publisher.publish(&envelope);

    assert_eq!(result.status(), SendStatus::ValidationFailed);
    assert_eq!(result.code(), "E_PAYLOAD_LIMIT");
}

#[test]
fn invalid_subscription_pattern_returns_stable_subject_validation_error() {
    let path = unique_socket("invalid-subscription");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let subscriber = Mesh::connect_with_options(autospawn_options(path))
        .expect("subscriber connects to local daemon");

    let err = subscriber
        .subscribe("mesh.>.created")
        .expect_err("invalid wildcard syntax is rejected");

    assert_eq!(err.code(), SdkErrorCode::SubjectValidation);
    assert!(err.to_string().contains("E_SUBJECT_VALIDATION"));
}

#[test]
fn oversized_inbound_frame_is_rejected_without_delivery() {
    let path = unique_socket("oversize-frame");
    let _cleanup = AutoSpawnCleanup::new(path.clone());
    let subscriber = Mesh::connect_with_options(autospawn_options(path.clone()))
        .expect("subscriber connects to local daemon");
    let mut subscription = subscriber
        .subscribe("mesh.trace.created")
        .expect("subscriber registers exact subject");
    let mut raw = UnixStream::connect(&path).expect("raw client connects to daemon socket");

    let oversize = u32::try_from(MAX_FRAME_BYTES + 1)
        .expect("test frame limit fits in u32")
        .to_be_bytes();
    raw.write_all(&oversize)
        .expect("raw client sends oversize frame length");
    let response = read_server_frame(&mut raw).expect("daemon rejects oversize frame");

    match response {
        ServerFrame::SendResult(result) => {
            assert_eq!(result.status(), FrameStatus::ValidationFailed);
            assert_eq!(result.code(), "E_PAYLOAD_LIMIT");
        }
        ServerFrame::Delivery { .. } => panic!("oversize frame must not produce a delivery"),
    }
    assert!(
        subscription
            .recv_delivery(Duration::from_millis(50))
            .expect("receive wait completes")
            .is_none(),
        "invalid inbound frame is not routed to subscribers"
    );
}
