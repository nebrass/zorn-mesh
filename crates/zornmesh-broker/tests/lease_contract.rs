use std::time::{Duration, SystemTime};

use zornmesh_broker::{Broker, FetchRequest, LeaseAckOutcome, LeaseErrorCode, LeaseRenewOutcome};
use zornmesh_core::{CoordinationOutcomeKind, CoordinationStage, Envelope, NackReasonCategory};

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn envelope(correlation: &str) -> Envelope {
    Envelope::with_metadata(
        "agent.local/producer",
        "agent.work.compute",
        b"{}".to_vec(),
        100,
        correlation.to_owned(),
        "application/json",
    )
    .expect("valid envelope")
}

#[test]
fn fetch_returns_leases_assigned_only_to_calling_consumer() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();
    broker.enqueue("queue.compute", envelope("e2")).unwrap();

    let fetched_a = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 10, Duration::from_secs(30)),
            at(10),
        )
        .expect("consumer A fetches");
    let fetched_b = broker
        .fetch_leases(
            FetchRequest::new("consumer.B", "queue.compute", 10, Duration::from_secs(30)),
            at(10),
        )
        .expect("consumer B fetches");

    assert_eq!(fetched_a.len(), 2);
    assert_eq!(fetched_b.len(), 0, "envelopes are not double-leased");
    for lease in &fetched_a {
        assert_eq!(lease.consumer_id(), "consumer.A");
        assert_eq!(lease.expiry(), at(40));
    }
}

#[test]
fn fetch_validation_rejects_invalid_batch_or_lease_duration_without_state_change() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let zero_batch = broker.fetch_leases(
        FetchRequest::new("consumer.A", "queue.compute", 0, Duration::from_secs(30)),
        at(10),
    );
    let zero_lease = broker.fetch_leases(
        FetchRequest::new("consumer.A", "queue.compute", 1, Duration::from_secs(0)),
        at(10),
    );

    assert!(matches!(
        zero_batch.as_ref().unwrap_err().code(),
        LeaseErrorCode::FetchValidation
    ));
    assert!(matches!(
        zero_lease.as_ref().unwrap_err().code(),
        LeaseErrorCode::FetchValidation
    ));
    assert_eq!(broker.queue_depth("queue.compute"), 1);
    assert_eq!(broker.active_lease_count(), 0);
}

#[test]
fn ack_marks_lease_terminal_and_envelope_is_not_refetched() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(30)),
            at(10),
        )
        .unwrap();
    let lease = &leases[0];

    let outcome = broker
        .ack_lease(lease.lease_id(), "consumer.A", at(11))
        .expect("ack succeeds");
    match outcome {
        LeaseAckOutcome::Acknowledged(coord) => {
            assert_eq!(coord.kind(), CoordinationOutcomeKind::Acknowledged);
            assert_eq!(coord.stage(), CoordinationStage::Delivery);
            assert!(coord.terminal());
        }
        other => panic!("expected Acknowledged, got {other:?}"),
    }

    let next = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(30)),
            at(12),
        )
        .unwrap();
    assert!(next.is_empty(), "ack'd envelope is not redelivered");
}

#[test]
fn nack_records_reason_and_returns_envelope_to_eligible_pool() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(30)),
            at(10),
        )
        .unwrap();
    let lease = &leases[0];

    let outcome = broker
        .nack_lease(
            lease.lease_id(),
            "consumer.A",
            NackReasonCategory::Transient,
            at(11),
        )
        .expect("nack succeeds");
    match outcome {
        LeaseAckOutcome::Nacked { outcome, reason } => {
            assert_eq!(outcome.kind(), CoordinationOutcomeKind::Rejected);
            assert_eq!(outcome.stage(), CoordinationStage::Delivery);
            assert!(outcome.terminal());
            assert_eq!(reason, NackReasonCategory::Transient);
        }
        other => panic!("expected Nacked, got {other:?}"),
    }

    let retry = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(30)),
            at(12),
        )
        .unwrap();
    assert_eq!(retry.len(), 1, "nacked envelope is eligible for retry");
    assert_eq!(retry[0].attempt(), 2);
}

#[test]
fn ack_or_nack_with_unknown_expired_or_foreign_lease_returns_stable_typed_outcomes() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(30)),
            at(10),
        )
        .unwrap();
    let lease = &leases[0];

    // unknown lease
    let unknown = broker.ack_lease("lease-does-not-exist", "consumer.A", at(11));
    assert!(matches!(
        unknown.as_ref().unwrap_err().code(),
        LeaseErrorCode::LeaseUnknown
    ));

    // foreign lease
    let foreign = broker.ack_lease(lease.lease_id(), "consumer.B", at(11));
    assert!(matches!(
        foreign.as_ref().unwrap_err().code(),
        LeaseErrorCode::LeaseNotOwned
    ));

    // ack then double-ack
    broker
        .ack_lease(lease.lease_id(), "consumer.A", at(11))
        .unwrap();
    let twice = broker.ack_lease(lease.lease_id(), "consumer.A", at(12));
    assert!(matches!(
        twice.as_ref().unwrap_err().code(),
        LeaseErrorCode::LeaseAlreadyTerminal
    ));

    // expired lease
    broker.enqueue("queue.compute", envelope("e2")).unwrap();
    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(5)),
            at(20),
        )
        .unwrap();
    let lease2 = &leases[0];
    let expired_now = at(30); // past 25s expiry
    broker.expire_due_leases(expired_now);
    let after_expiry = broker.ack_lease(lease2.lease_id(), "consumer.A", expired_now);
    assert!(matches!(
        after_expiry.as_ref().unwrap_err().code(),
        LeaseErrorCode::LeaseExpired
    ));
}

#[test]
fn renew_extends_lease_without_duplicating_delivery() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 5, Duration::from_secs(10)),
            at(10),
        )
        .unwrap();
    let lease = &leases[0];

    let renewal = broker
        .renew_lease(
            lease.lease_id(),
            "consumer.A",
            Duration::from_secs(20),
            at(15),
        )
        .expect("renewal succeeds");
    let LeaseRenewOutcome::Renewed { new_expiry } = renewal;
    assert_eq!(new_expiry, at(35));
    assert_eq!(broker.active_lease_count(), 1, "no duplication on renew");

    // renew on unknown lease fails
    let bad = broker.renew_lease("missing", "consumer.A", Duration::from_secs(5), at(16));
    assert!(matches!(
        bad.as_ref().unwrap_err().code(),
        LeaseErrorCode::LeaseUnknown
    ));
}

#[test]
fn lease_expiry_makes_envelope_refetchable_with_attempt_metadata() {
    let broker = Broker::new();
    broker.enqueue("queue.compute", envelope("e1")).unwrap();

    let leases = broker
        .fetch_leases(
            FetchRequest::new("consumer.A", "queue.compute", 1, Duration::from_secs(5)),
            at(10),
        )
        .unwrap();
    let lease = &leases[0];
    assert_eq!(lease.attempt(), 1);

    let expired = broker.expire_due_leases(at(20));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].lease_id(), lease.lease_id());
    assert_eq!(expired[0].attempt(), 1);

    let refetch = broker
        .fetch_leases(
            FetchRequest::new("consumer.B", "queue.compute", 1, Duration::from_secs(5)),
            at(21),
        )
        .unwrap();
    assert_eq!(refetch.len(), 1);
    assert_eq!(
        refetch[0].attempt(),
        2,
        "retry attempts visible in lease metadata"
    );
}

#[test]
fn fixture_pins_lease_error_codes_and_audit_kinds() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "lease|error|E_FETCH_VALIDATION",
        "lease|error|E_LEASE_UNKNOWN",
        "lease|error|E_LEASE_NOT_OWNED",
        "lease|error|E_LEASE_EXPIRED",
        "lease|error|E_LEASE_ALREADY_TERMINAL",
        "lease|audit|acknowledged",
        "lease|audit|nacked",
        "lease|audit|renewed",
        "lease|audit|expired",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }

    assert_eq!(
        LeaseErrorCode::FetchValidation.as_str(),
        "E_FETCH_VALIDATION"
    );
    assert_eq!(LeaseErrorCode::LeaseUnknown.as_str(), "E_LEASE_UNKNOWN");
    assert_eq!(LeaseErrorCode::LeaseNotOwned.as_str(), "E_LEASE_NOT_OWNED");
    assert_eq!(LeaseErrorCode::LeaseExpired.as_str(), "E_LEASE_EXPIRED");
    assert_eq!(
        LeaseErrorCode::LeaseAlreadyTerminal.as_str(),
        "E_LEASE_ALREADY_TERMINAL"
    );
}
