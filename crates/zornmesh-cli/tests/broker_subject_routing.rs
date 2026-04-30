use std::sync::mpsc;

use zornmesh_cli::broker::{Broker, BrokerErrorCode, SubjectPattern};
use zornmesh_cli::core::Envelope;

#[test]
fn subject_matching_uses_shared_conformance_fixture() {
    let fixture = include_str!("../../../fixtures/pubsub/subject-routing.txt");

    for (index, line) in fixture.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts = line.split('|').collect::<Vec<_>>();
        assert_eq!(parts.len(), 4, "fixture line {} has four fields", index + 1);
        let pattern = SubjectPattern::new(parts[1]).expect("fixture pattern is valid");
        let expected = parts[3]
            .parse::<bool>()
            .expect("fixture match flag is bool");

        assert_eq!(
            pattern.matches(parts[2]),
            expected,
            "fixture line {} ({})",
            index + 1,
            parts[0]
        );
    }
}

#[test]
fn invalid_subject_patterns_are_rejected_without_registering_routes() {
    let broker = Broker::new();

    for invalid in [
        "mesh.trace.created.",
        "mesh.trace.*suffix",
        "mesh.>.created",
        "zorn.internal.created",
        "mesh.one.two.three.four.five.six.seven.eight",
    ] {
        let (tx, _rx) = mpsc::channel();
        let err = broker
            .subscribe(invalid, tx)
            .expect_err("invalid pattern is rejected");

        assert_eq!(err.code(), BrokerErrorCode::SubjectValidation);
        assert_eq!(broker.subscription_count(), 0);
    }

    let too_long = "a".repeat(257);
    let (tx, _rx) = mpsc::channel();
    let err = broker
        .subscribe(&too_long, tx)
        .expect_err("too-long pattern is rejected");

    assert_eq!(err.code(), BrokerErrorCode::SubjectValidation);
    assert_eq!(broker.subscription_count(), 0);
}

#[test]
fn subscriber_caps_are_enforced_before_state_is_retained() {
    let broker = Broker::new();
    let mut subscriptions = Vec::new();

    for _ in 0..256 {
        let (tx, _rx) = mpsc::channel();
        subscriptions.push(
            broker
                .subscribe("mesh.trace.created", tx)
                .expect("subscriber within per-pattern cap is accepted"),
        );
    }

    let (tx, _rx) = mpsc::channel();
    let err = broker
        .subscribe("mesh.trace.created", tx)
        .expect_err("subscriber cap is enforced");

    assert_eq!(err.code(), BrokerErrorCode::SubscriptionCap);
    assert_eq!(broker.subscription_count(), 256);
}

#[test]
fn total_subscription_cap_is_enforced_before_state_is_retained() {
    let broker = Broker::new();
    let mut subscriptions = Vec::new();

    for index in 0..4096 {
        let (tx, _rx) = mpsc::channel();
        subscriptions.push(
            broker
                .subscribe(format!("mesh.trace.subject{index}"), tx)
                .expect("subscriber within total cap is accepted"),
        );
    }

    let (tx, _rx) = mpsc::channel();
    let err = broker
        .subscribe("mesh.trace.overflow", tx)
        .expect_err("total subscription cap is enforced");

    assert_eq!(err.code(), BrokerErrorCode::SubscriptionCap);
    assert_eq!(broker.subscription_count(), 4096);
}

#[test]
fn publish_delivers_one_attempt_to_matching_subscribers_only() {
    let broker = Broker::new();
    let (matching_tx, matching_rx) = mpsc::channel();
    let (nonmatching_tx, nonmatching_rx) = mpsc::channel();

    let _matching_subscription = broker
        .subscribe("mesh.trace.>", matching_tx)
        .expect("prefix subscription registers");
    let _nonmatching_subscription = broker
        .subscribe("mesh.audit.created", nonmatching_tx)
        .expect("non-matching subscription registers");

    let envelope = Envelope::new(
        "agent.local/publisher",
        "mesh.trace.created",
        b"{\"trace_id\":\"trace-1\"}".to_vec(),
    )
    .expect("valid envelope");

    let delivery_count = broker
        .publish(envelope.clone())
        .expect("publish is accepted");

    assert_eq!(delivery_count, 1);
    let delivery = matching_rx
        .try_recv()
        .expect("matching subscriber receives");
    assert_eq!(delivery.attempt(), 1);
    assert_eq!(delivery.envelope().source_agent(), envelope.source_agent());
    assert_eq!(delivery.envelope().subject(), envelope.subject());
    assert_eq!(
        delivery.envelope().correlation_id(),
        envelope.correlation_id()
    );
    assert_eq!(delivery.envelope().payload(), envelope.payload());
    assert_eq!(
        delivery.envelope().trace_context().trace_id(),
        envelope.trace_context().trace_id()
    );
    assert_ne!(
        delivery.envelope().trace_context().span_id(),
        envelope.trace_context().span_id()
    );
    assert!(
        nonmatching_rx.try_recv().is_err(),
        "non-matching subscriber receives nothing"
    );
}
