use std::time::Duration;

use zornmesh_broker::{
    BackpressureDetails, BackpressureOutcome, Broker, ConsumerHealthSignal, ConsumerHealthState,
    QueueBoundsConfig, QueueDropPolicy,
};
use zornmesh_core::Envelope;

fn envelope(correlation: &str) -> Envelope {
    Envelope::with_metadata(
        "agent.local/producer",
        "agent.work.compute",
        b"secret-payload-must-not-leak".to_vec(),
        100,
        correlation.to_owned(),
        "application/json",
    )
    .expect("valid envelope")
}

#[test]
fn queue_bound_reached_returns_publisher_visible_backpressure_with_safe_details() {
    let broker = Broker::new();
    broker
        .configure_queue_bounds(
            "queue.compute",
            QueueBoundsConfig::new(2, QueueDropPolicy::Reject),
        )
        .unwrap();

    broker
        .publish_with_backpressure("queue.compute", envelope("e1"))
        .unwrap();
    broker
        .publish_with_backpressure("queue.compute", envelope("e2"))
        .unwrap();

    let outcome = broker
        .publish_with_backpressure("queue.compute", envelope("e3"))
        .expect("backpressure outcome returns");
    match outcome {
        BackpressureOutcome::RejectedBackpressure { details } => {
            assert_eq!(details.subject_scope(), "queue.compute");
            assert_eq!(details.queue_bound(), 2);
            assert_eq!(details.exceeded_limit(), 2);
            assert!(details.retryable());
            assert!(details.suggested_delay() >= Duration::from_millis(1));
            // Safety: details type has no payload field; this is enforced by the type
            // system. We additionally verify the message string contains no secret bytes.
            assert!(!details.remediation().contains("secret-payload"));
        }
        other => panic!("expected RejectedBackpressure, got {other:?}"),
    }
}

#[test]
fn drop_policy_drop_oldest_evicts_first_envelope_and_reports_policy() {
    let broker = Broker::new();
    broker
        .configure_queue_bounds(
            "queue.compute",
            QueueBoundsConfig::new(2, QueueDropPolicy::DropOldest),
        )
        .unwrap();

    broker
        .publish_with_backpressure("queue.compute", envelope("e1"))
        .unwrap();
    broker
        .publish_with_backpressure("queue.compute", envelope("e2"))
        .unwrap();

    let outcome = broker
        .publish_with_backpressure("queue.compute", envelope("e3"))
        .unwrap();
    match outcome {
        BackpressureOutcome::DroppedByPolicy {
            policy,
            details,
            dropped_correlation_id,
        } => {
            assert_eq!(policy, QueueDropPolicy::DropOldest);
            assert_eq!(details.queue_bound(), 2);
            assert_eq!(dropped_correlation_id.as_deref(), Some("e1"));
        }
        other => panic!("expected DroppedByPolicy, got {other:?}"),
    }
    assert_eq!(broker.queue_depth("queue.compute"), 2);
}

#[test]
fn drop_policy_drop_newest_rejects_incoming_and_reports_policy() {
    let broker = Broker::new();
    broker
        .configure_queue_bounds(
            "queue.compute",
            QueueBoundsConfig::new(1, QueueDropPolicy::DropNewest),
        )
        .unwrap();
    broker
        .publish_with_backpressure("queue.compute", envelope("e1"))
        .unwrap();

    let outcome = broker
        .publish_with_backpressure("queue.compute", envelope("e2"))
        .unwrap();
    match outcome {
        BackpressureOutcome::DroppedByPolicy {
            policy,
            dropped_correlation_id,
            ..
        } => {
            assert_eq!(policy, QueueDropPolicy::DropNewest);
            assert_eq!(dropped_correlation_id.as_deref(), Some("e2"));
        }
        other => panic!("expected DroppedByPolicy, got {other:?}"),
    }
    assert_eq!(broker.queue_depth("queue.compute"), 1);
}

#[test]
fn consecutive_missed_signals_promote_consumer_to_backpressured_then_failed() {
    let broker = Broker::new();
    let id = "consumer.A";

    assert_eq!(
        broker.consumer_health_state(id),
        ConsumerHealthState::Healthy
    );

    let s1 = broker
        .record_consumer_health_signal(id, ConsumerHealthSignal::MissedAck)
        .unwrap();
    assert_eq!(s1, ConsumerHealthState::Backpressured);

    let s2 = broker
        .record_consumer_health_signal(id, ConsumerHealthSignal::MissedLease)
        .unwrap();
    assert_eq!(s2, ConsumerHealthState::Retrying);

    let s3 = broker
        .record_consumer_health_signal(id, ConsumerHealthSignal::MissedAck)
        .unwrap();
    assert_eq!(s3, ConsumerHealthState::Failed);
}

#[test]
fn clearing_consumer_backpressure_restores_healthy_state() {
    let broker = Broker::new();
    let id = "consumer.A";

    broker
        .record_consumer_health_signal(id, ConsumerHealthSignal::MissedAck)
        .unwrap();
    broker
        .record_consumer_health_signal(id, ConsumerHealthSignal::MissedLease)
        .unwrap();
    assert!(matches!(
        broker.consumer_health_state(id),
        ConsumerHealthState::Retrying
    ));

    broker.clear_consumer_backpressure(id);
    assert_eq!(
        broker.consumer_health_state(id),
        ConsumerHealthState::Healthy
    );

    // Subsequent publish doesn't carry stale state.
    broker
        .configure_queue_bounds(
            "queue.compute",
            QueueBoundsConfig::new(8, QueueDropPolicy::Reject),
        )
        .unwrap();
    let outcome = broker
        .publish_with_backpressure("queue.compute", envelope("ok"))
        .unwrap();
    assert!(matches!(outcome, BackpressureOutcome::Accepted));
}

#[test]
fn unconfigured_queue_uses_default_bounds_and_accepts_publishes() {
    let broker = Broker::new();
    let outcome = broker
        .publish_with_backpressure("queue.unbound", envelope("e1"))
        .unwrap();
    assert!(matches!(outcome, BackpressureOutcome::Accepted));
}

#[test]
fn invalid_queue_bounds_configuration_is_rejected_with_validation() {
    let broker = Broker::new();
    let err = broker
        .configure_queue_bounds(
            "queue.compute",
            QueueBoundsConfig::new(0, QueueDropPolicy::Reject),
        )
        .unwrap_err();
    assert_eq!(err.code().as_str(), "E_DELIVERY_VALIDATION");
}

#[test]
fn fixture_pins_backpressure_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "backpressure|outcome|accepted",
        "backpressure|outcome|deferred",
        "backpressure|outcome|rejected_backpressure",
        "backpressure|outcome|dropped_by_policy",
        "backpressure|policy|reject",
        "backpressure|policy|drop_oldest",
        "backpressure|policy|drop_newest",
        "consumer_health|state|healthy",
        "consumer_health|state|backpressured",
        "consumer_health|state|retrying",
        "consumer_health|state|failed",
        "consumer_health|signal|missed_ack",
        "consumer_health|signal|missed_lease",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}

fn _ensure_safety_marker(_d: &BackpressureDetails) {
    // BackpressureDetails has no payload field; this is enforced by
    // the type system. This helper marker keeps the type referenced.
}
