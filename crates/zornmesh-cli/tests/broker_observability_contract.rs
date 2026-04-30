use std::{
    sync::mpsc,
    time::{Duration, SystemTime},
};

use zornmesh_cli::broker::{
    Broker, ChunkSubmission, FetchRequest, ReplySubmissionOutcome, RequestRegistration,
    StreamFinality, StreamRegistration,
};
use zornmesh_cli::core::{
    Envelope, LocalTelemetry, NackReasonCategory, TELEMETRY_SCHEMA_VERSION, TraceContext,
};

const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn envelope(correlation_id: &str) -> Envelope {
    Envelope::with_trace_context(
        "agent.local/source",
        "mesh.work.created",
        b"{}".to_vec(),
        42,
        correlation_id,
        "application/json",
        TRACEPARENT,
        Some("rojo=00f067aa0ba902b7"),
    )
    .expect("valid traced envelope")
}

#[test]
fn publish_delivery_and_ack_propagate_trace_context_and_emit_schema_safe_telemetry() {
    let telemetry = LocalTelemetry::default();
    let broker = Broker::with_telemetry(telemetry.clone());
    let (tx, rx) = mpsc::channel();
    let _subscription = broker
        .subscribe("mesh.work.>", tx)
        .expect("subscription accepted");
    let envelope = envelope("corr-pub-1");
    let incoming_span = envelope.trace_context().span_id().to_owned();

    let receipt = broker.publish(envelope.clone()).expect("publish succeeds");
    assert_eq!(receipt.delivery_attempts(), 1);
    let delivery = rx
        .recv_timeout(Duration::from_millis(50))
        .expect("delivery received");
    assert_eq!(
        delivery.envelope().trace_context().trace_id(),
        envelope.trace_context().trace_id()
    );
    assert_ne!(delivery.envelope().trace_context().span_id(), incoming_span);

    broker
        .record_ack(delivery.delivery_id())
        .expect("delivery ack succeeds");

    let spans = telemetry.spans();
    for name in [
        "zornmesh.publish.route",
        "zornmesh.publish.deliver",
        "zornmesh.delivery.ack",
    ] {
        let span = spans
            .iter()
            .find(|span| span.name() == name)
            .unwrap_or_else(|| panic!("missing span {name}; spans={spans:?}"));
        assert_eq!(span.schema_version(), TELEMETRY_SCHEMA_VERSION);
        assert_eq!(span.trace_id(), envelope.trace_context().trace_id());
    }
    assert!(
        spans
            .iter()
            .any(|span| span.name() == "zornmesh.publish.route"
                && span.parent_span_id() == Some(incoming_span.as_str()))
    );

    for metric in telemetry.metrics() {
        assert!(metric.name().starts_with("zornmesh."));
        for label in metric.labels() {
            assert_ne!(label.key(), "correlation_id");
            assert_ne!(label.key(), "trace_id");
            assert_ne!(label.key(), "message_id");
            assert_ne!(label.key(), "subject");
        }
    }
}

#[test]
fn request_stream_fetch_retry_and_cancellation_states_are_explicit_span_events() {
    let telemetry = LocalTelemetry::default();
    let broker = Broker::with_telemetry(telemetry.clone());
    let context = TraceContext::from_w3c(TRACEPARENT, None).expect("valid trace context");

    let registration = RequestRegistration::new(
        "corr-request-1",
        "agent.local/source",
        "agent.local/target",
        "mesh.work.compute",
        Duration::from_secs(30),
    )
    .with_trace_context(context.clone());
    let _handle = broker
        .register_request(registration, at(10))
        .expect("request registers");
    broker
        .cancel_request("corr-request-1", at(11))
        .expect("request cancellation records");

    broker
        .open_stream(
            StreamRegistration::new(
                "stream-1",
                "corr-stream-1",
                "agent.local/source",
                "agent.local/target",
                4096,
                16_384,
            )
            .with_trace_context(context.clone()),
        )
        .expect("stream opens");
    broker
        .submit_chunk(ChunkSubmission::new(
            "stream-1",
            0,
            vec![0; 128],
            StreamFinality::Continue,
        ))
        .expect("chunk accepted");
    broker
        .cancel_stream_by_correlation("corr-stream-1")
        .expect("stream cancellation records");

    broker
        .enqueue("queue.work", envelope("corr-lease-1"))
        .unwrap();
    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.work", 1, Duration::from_secs(5)),
            at(20),
        )
        .expect("lease fetch succeeds");
    let lease = &leases[0];
    broker
        .nack_lease(
            lease.lease_id(),
            "consumer.A",
            NackReasonCategory::Transient,
            at(21),
        )
        .expect("nack records retry");
    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.B", "queue.work", 1, Duration::from_secs(5)),
            at(22),
        )
        .expect("retry fetch succeeds");
    assert_eq!(leases[0].attempt(), 2);

    let spans = telemetry.spans();
    for name in [
        "zornmesh.request.register",
        "zornmesh.request.cancel",
        "zornmesh.stream.open",
        "zornmesh.stream.chunk",
        "zornmesh.stream.cancel",
        "zornmesh.lease.fetch",
        "zornmesh.lease.nack",
    ] {
        assert!(
            spans.iter().any(|span| span.name() == name),
            "missing span {name}; spans={spans:?}"
        );
    }
    assert!(spans.iter().any(|span| span.name() == "zornmesh.lease.nack"
        && span.events().iter().any(|event| event == "retry")));
    assert!(
        spans
            .iter()
            .any(|span| span.name() == "zornmesh.request.cancel"
                && span.events().iter().any(|event| event == "cancellation"))
    );
}

#[test]
fn late_replies_emit_schema_spans_that_preserve_timed_out_request_causality() {
    let telemetry = LocalTelemetry::default();
    let broker = Broker::with_telemetry(telemetry.clone());
    let context = TraceContext::from_w3c(TRACEPARENT, None).expect("valid trace context");

    let registration = RequestRegistration::new(
        "corr-late-1",
        "agent.local/source",
        "agent.local/target",
        "mesh.work.compute",
        Duration::from_millis(500),
    )
    .with_trace_context(context.clone());
    let _handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let expired = broker.tick_request_timeouts(at(11));
    assert_eq!(expired.len(), 1);
    let outcome = broker
        .submit_reply("corr-late-1", envelope("corr-late-1"), at(12))
        .expect("late reply returns a typed outcome");
    assert!(matches!(outcome, ReplySubmissionOutcome::LateAfterTimeout));

    let spans = telemetry.spans();
    let timeout_span = spans
        .iter()
        .find(|span| span.name() == "zornmesh.request.timeout")
        .expect("timeout span recorded");
    let late_span = spans
        .iter()
        .find(|span| {
            span.name() == "zornmesh.request.reply"
                && span
                    .events()
                    .iter()
                    .any(|event| event == "late_after_timeout")
        })
        .unwrap_or_else(|| panic!("missing late reply span; spans={spans:?}"));

    assert_eq!(late_span.schema_version(), TELEMETRY_SCHEMA_VERSION);
    assert_eq!(late_span.trace_id(), context.trace_id());
    assert_eq!(late_span.parent_span_id(), Some(timeout_span.span_id()));
    assert!(late_span.attributes().iter().any(|attribute| {
        attribute.key() == "delivery.state" && attribute.value() == "late_after_timeout"
    }));
}
