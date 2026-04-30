use zornmesh_cli::core::{
    COORDINATION_CONTRACT_VERSION, CoordinationOutcome, CoordinationOutcomeKind, CoordinationStage,
    DELIVERY_STATE_TAXONOMY_VERSION, ENVELOPE_SCHEMA_VERSION, ERROR_CONTRACT_VERSION,
    ErrorCategory, NackReasonCategory, ProductError, TELEMETRY_OVERFLOW_LABEL,
    TELEMETRY_SCHEMA_VERSION,
};

#[test]
fn fixture_pins_coordination_versions_and_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");

    for expected in [
        "version|coordination|zornmesh.coordination.v1",
        "version|envelope-schema|zornmesh.envelope.v1",
        "version|error-contract|zornmesh.error.v1",
        "version|delivery-state-taxonomy|zornmesh.delivery-state.v1",
        "version|telemetry-schema|zornmesh.telemetry.v1",
        "trace_context|rule|generate_missing",
        "trace_context|rule|reject_malformed",
        "telemetry|span|zornmesh.publish.route",
        "telemetry|span|zornmesh.lease.nack",
        "telemetry|metric|zornmesh.delivery.attempts",
        "telemetry|label_forbidden|correlation_id",
        "telemetry|label_forbidden|subject",
        "telemetry|exporter_failure|OTEL_EXPORTER_SLOW",
    ] {
        assert!(fixture.contains(expected), "missing fixture row {expected}");
    }
    assert_eq!(COORDINATION_CONTRACT_VERSION, "zornmesh.coordination.v1");
    assert_eq!(ENVELOPE_SCHEMA_VERSION, "zornmesh.envelope.v1");
    assert_eq!(ERROR_CONTRACT_VERSION, "zornmesh.error.v1");
    assert_eq!(
        DELIVERY_STATE_TAXONOMY_VERSION,
        "zornmesh.delivery-state.v1"
    );
    assert_eq!(TELEMETRY_SCHEMA_VERSION, "zornmesh.telemetry.v1");
    assert_eq!(TELEMETRY_OVERFLOW_LABEL, "__overflow__");

    for (kind, stage, retryable, terminal) in [
        (
            CoordinationOutcomeKind::Accepted,
            CoordinationStage::Transport,
            false,
            false,
        ),
        (
            CoordinationOutcomeKind::DurableAccepted,
            CoordinationStage::Durable,
            false,
            false,
        ),
        (
            CoordinationOutcomeKind::Acknowledged,
            CoordinationStage::Delivery,
            false,
            true,
        ),
        (
            CoordinationOutcomeKind::Rejected,
            CoordinationStage::Delivery,
            false,
            true,
        ),
        (
            CoordinationOutcomeKind::Failed,
            CoordinationStage::Delivery,
            false,
            true,
        ),
        (
            CoordinationOutcomeKind::TimedOut,
            CoordinationStage::Transport,
            true,
            true,
        ),
        (
            CoordinationOutcomeKind::Retryable,
            CoordinationStage::Transport,
            true,
            false,
        ),
        (
            CoordinationOutcomeKind::Terminal,
            CoordinationStage::Transport,
            false,
            true,
        ),
    ] {
        let row = format!(
            "outcome|{}|{}|{}|{}",
            kind.as_str(),
            stage.as_str(),
            retryable,
            terminal
        );
        assert!(fixture.contains(&row), "missing fixture row {row}");
        let outcome =
            CoordinationOutcome::new(kind, stage, "TEST", "test outcome", retryable, terminal, 0);
        assert_eq!(outcome.version(), COORDINATION_CONTRACT_VERSION);
        assert_eq!(outcome.kind(), kind);
        assert_eq!(outcome.stage(), stage);
        assert_eq!(outcome.retryable(), retryable);
        assert_eq!(outcome.terminal(), terminal);
    }
}

#[test]
fn nack_reason_categories_are_safe_wire_values() {
    let reasons = [
        NackReasonCategory::Validation,
        NackReasonCategory::Authorization,
        NackReasonCategory::Processing,
        NackReasonCategory::Timeout,
        NackReasonCategory::PayloadLimit,
        NackReasonCategory::Backpressure,
        NackReasonCategory::Transient,
        NackReasonCategory::Policy,
        NackReasonCategory::Unknown,
    ];

    for reason in reasons {
        let wire = reason.as_str();
        assert!(
            wire.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'),
            "{wire} must be safe for fixtures and logs"
        );
        assert_eq!(NackReasonCategory::from_wire(wire), Some(reason));
    }
}

#[test]
fn product_errors_expose_stable_code_category_retryability_and_safe_details() {
    let cases = [
        (
            ProductError::new(
                "E_SUBJECT_VALIDATION",
                ErrorCategory::Validation,
                false,
                "subject contains wildcard",
            ),
            "error|E_SUBJECT_VALIDATION|validation|false",
        ),
        (
            ProductError::new(
                "E_DAEMON_UNREACHABLE",
                ErrorCategory::Reachability,
                true,
                "daemon socket is not reachable",
            ),
            "error|E_DAEMON_UNREACHABLE|reachability|true",
        ),
        (
            ProductError::new(
                "E_PERSISTENCE_UNAVAILABLE",
                ErrorCategory::PersistenceUnavailable,
                false,
                "durable coordination state is unavailable for this store",
            ),
            "error|E_PERSISTENCE_UNAVAILABLE|persistence_unavailable|false",
        ),
    ];
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");

    for (error, row) in cases {
        assert!(fixture.contains(row), "missing fixture row {row}");
        assert_eq!(error.version(), ERROR_CONTRACT_VERSION);
        assert!(!error.code().is_empty());
        assert!(!error.safe_details().is_empty());
        assert!(!error.safe_details().contains('{'));
        assert!(!error.safe_details().contains('}'));
    }
}
