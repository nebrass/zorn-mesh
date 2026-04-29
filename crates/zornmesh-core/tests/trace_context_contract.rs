use zornmesh_core::{
    Envelope, LocalTelemetry, TELEMETRY_CARDINALITY_LIMIT, TELEMETRY_OVERFLOW_LABEL,
    TELEMETRY_SCHEMA_VERSION, TelemetryExporterFailure, TelemetryLabel, TelemetryMetric,
    TelemetrySpan, TraceContext,
};

const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

#[test]
fn w3c_trace_context_is_validated_and_preserved_on_envelopes() {
    let envelope = Envelope::with_trace_context(
        "agent.local/source",
        "mesh.work.created",
        b"{}".to_vec(),
        42,
        "corr-trace-1",
        "application/json",
        TRACEPARENT,
        Some("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE"),
    )
    .expect("valid W3C trace context");

    assert_eq!(envelope.trace_context().traceparent(), TRACEPARENT);
    assert_eq!(
        envelope.trace_context().tracestate(),
        Some("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE")
    );
    assert_eq!(
        envelope.trace_context().trace_id(),
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
    assert_eq!(envelope.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
}

#[test]
fn missing_trace_context_is_generated_as_valid_w3c_context() {
    let envelope = Envelope::new("agent.local/source", "mesh.work.created", b"{}".to_vec())
        .expect("valid envelope");
    let generated = envelope.trace_context();

    assert!(generated.traceparent().starts_with("00-"));
    assert_eq!(generated.traceparent().len(), TRACEPARENT.len());
    assert_eq!(generated.trace_id().len(), 32);
    assert_eq!(generated.span_id().len(), 16);
    assert!(
        generated
            .trace_id()
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    );
    assert!(generated.span_id().chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_ne!(generated.trace_id(), "00000000000000000000000000000000");
    assert_ne!(generated.span_id(), "0000000000000000");
    assert_eq!(generated.tracestate(), None);
}

#[test]
fn malformed_traceparent_or_tracestate_is_rejected_before_propagation() {
    let zero_trace_id = "00-00000000000000000000000000000000-00f067aa0ba902b7-01";
    let zero_span_id = "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01";
    let bad_version = "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    for invalid in [
        zero_trace_id,
        zero_span_id,
        bad_version,
        "not-a-traceparent",
    ] {
        let err = TraceContext::from_w3c(invalid, None).expect_err("invalid traceparent rejected");
        assert_eq!(err.code(), "E_TRACE_CONTEXT_VALIDATION");
    }

    let err = TraceContext::from_w3c(TRACEPARENT, Some("vendor=value\nbad=true"))
        .expect_err("tracestate with control chars is rejected");
    assert_eq!(err.code(), "E_TRACE_CONTEXT_VALIDATION");
}

#[test]
fn telemetry_schema_sanitizes_metrics_and_records_exporter_diagnostics_locally() {
    let telemetry = LocalTelemetry::default();
    let context = TraceContext::from_w3c(TRACEPARENT, None).expect("valid context");
    let child = context.child();

    telemetry.record_span(
        TelemetrySpan::new("zornmesh.publish.route", &child, Some(context.span_id()))
            .with_attribute("delivery.state", "accepted")
            .with_event("routed"),
    );
    telemetry.record_metric(TelemetryMetric::new(
        "zornmesh.delivery.attempts",
        1,
        vec![
            TelemetryLabel::new("agent", "agent.local/source"),
            TelemetryLabel::new("delivery_state", "accepted"),
            TelemetryLabel::new("correlation_id", "corr-never-a-label"),
            TelemetryLabel::new("subject", "mesh.raw.subject.never.a.label"),
        ],
    ));
    for index in 0..=TELEMETRY_CARDINALITY_LIMIT {
        telemetry.record_metric(TelemetryMetric::new(
            "zornmesh.agent.events",
            1,
            vec![TelemetryLabel::new("agent", format!("agent-{index}"))],
        ));
    }
    telemetry.record_exporter_failure(
        TelemetryExporterFailure::Slow,
        "loopback exporter timed out",
    );

    let spans = telemetry.spans();
    assert_eq!(spans[0].schema_version(), TELEMETRY_SCHEMA_VERSION);
    assert_eq!(spans[0].name(), "zornmesh.publish.route");
    assert_eq!(spans[0].trace_id(), context.trace_id());
    assert_eq!(spans[0].parent_span_id(), Some(context.span_id()));
    assert_eq!(spans[0].events(), &["routed"]);

    let metrics = telemetry.metrics();
    let delivery_labels = metrics[0].labels();
    assert!(delivery_labels.iter().any(|label| label.key() == "agent"));
    assert!(
        delivery_labels
            .iter()
            .all(|label| label.key() != "correlation_id" && label.key() != "subject")
    );
    assert!(metrics.iter().any(|metric| {
        metric
            .labels()
            .iter()
            .any(|label| label.value() == TELEMETRY_OVERFLOW_LABEL)
    }));

    let diagnostics = telemetry.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].schema_version(), TELEMETRY_SCHEMA_VERSION);
    assert_eq!(diagnostics[0].code(), "OTEL_EXPORTER_SLOW");
}
