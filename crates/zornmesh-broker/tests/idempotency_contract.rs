use std::time::{Duration, SystemTime};

use zornmesh_broker::{
    Broker, IdempotencyConflictReason, IdempotencyDecision, IdempotencyError, IdempotencyErrorCode,
    IdempotencyRequest, IdempotencySendOutcome,
};
use zornmesh_core::{CoordinationOutcome, CoordinationOutcomeKind};

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn req(
    sender: &str,
    key: &str,
    subject: &str,
    fingerprint: &str,
    correlation_id: &str,
) -> IdempotencyRequest {
    IdempotencyRequest::new(sender, key, subject, fingerprint, "send", correlation_id)
        .with_trace_context("trace-root-1")
        .with_timeout(Duration::from_secs(5))
}

fn accepted_outcome() -> CoordinationOutcome {
    CoordinationOutcome::accepted("accepted by broker", 1)
}

#[test]
fn first_attempt_is_recorded_pending_and_subsequent_duplicate_with_known_outcome_is_deduped() {
    let broker = Broker::new();
    let request = req(
        "agent.local/a",
        "key-1",
        "agent.work.compute",
        "sha256:abc",
        "corr-1",
    );

    let first = broker
        .register_send(request.clone(), at(10))
        .expect("first attempt registers");
    assert!(matches!(first, IdempotencyDecision::FirstAttempt));

    broker
        .commit_send(
            "agent.local/a",
            "key-1",
            IdempotencySendOutcome::Accepted(accepted_outcome()),
        )
        .expect("commit succeeds");

    let second = broker
        .register_send(request.clone(), at(11))
        .expect("second attempt evaluated");
    match second {
        IdempotencyDecision::Deduplicated {
            original_outcome,
            correlation_id,
            trace_context,
        } => {
            assert_eq!(original_outcome.kind(), CoordinationOutcomeKind::Accepted);
            assert_eq!(correlation_id, "corr-1");
            assert_eq!(trace_context.as_deref(), Some("trace-root-1"));
        }
        other => panic!("expected Deduplicated, got {other:?}"),
    }
}

#[test]
fn fingerprint_mismatch_returns_idempotency_conflict_without_routing_new_work() {
    let broker = Broker::new();
    let original = req(
        "agent.local/a",
        "key-shared",
        "agent.work.compute",
        "sha256:abc",
        "corr-1",
    );
    let conflicting_subject = req(
        "agent.local/a",
        "key-shared",
        "agent.work.other",
        "sha256:abc",
        "corr-2",
    );
    let conflicting_payload = req(
        "agent.local/a",
        "key-shared",
        "agent.work.compute",
        "sha256:zzz",
        "corr-3",
    );

    broker.register_send(original.clone(), at(10)).unwrap();
    broker
        .commit_send(
            "agent.local/a",
            "key-shared",
            IdempotencySendOutcome::Accepted(accepted_outcome()),
        )
        .unwrap();

    let subject_conflict = broker.register_send(conflicting_subject, at(11)).unwrap();
    match subject_conflict {
        IdempotencyDecision::Conflict { reason } => {
            assert_eq!(reason, IdempotencyConflictReason::SubjectMismatch);
        }
        other => panic!("expected SubjectMismatch conflict, got {other:?}"),
    }

    let payload_conflict = broker.register_send(conflicting_payload, at(12)).unwrap();
    match payload_conflict {
        IdempotencyDecision::Conflict { reason } => {
            assert_eq!(
                reason,
                IdempotencyConflictReason::PayloadFingerprintMismatch
            );
        }
        other => panic!("expected PayloadFingerprintMismatch conflict, got {other:?}"),
    }
}

#[test]
fn different_senders_using_same_key_do_not_collide() {
    let broker = Broker::new();
    let a = req(
        "agent.local/a",
        "key-shared",
        "agent.work.compute",
        "sha256:abc",
        "corr-1",
    );
    let b = req(
        "agent.local/b",
        "key-shared",
        "agent.work.other",
        "sha256:xyz",
        "corr-2",
    );

    let a1 = broker.register_send(a.clone(), at(10)).unwrap();
    let b1 = broker.register_send(b.clone(), at(10)).unwrap();
    assert!(matches!(a1, IdempotencyDecision::FirstAttempt));
    assert!(matches!(b1, IdempotencyDecision::FirstAttempt));
}

#[test]
fn retry_after_transport_failure_before_commit_returns_unknown_outcome() {
    let broker = Broker::new();
    let request = req(
        "agent.local/a",
        "key-pending",
        "agent.work.compute",
        "sha256:abc",
        "corr-1",
    );

    let first = broker.register_send(request.clone(), at(10)).unwrap();
    assert!(matches!(first, IdempotencyDecision::FirstAttempt));

    // SDK never reached commit_send because transport failed.
    let retry = broker.register_send(request.clone(), at(11)).unwrap();
    match retry {
        IdempotencyDecision::Unknown {
            correlation_id,
            trace_context,
        } => {
            assert_eq!(correlation_id, "corr-1");
            assert_eq!(trace_context.as_deref(), Some("trace-root-1"));
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn commit_send_for_unknown_or_already_committed_record_returns_typed_error() {
    let broker = Broker::new();
    let unknown = broker.commit_send(
        "agent.local/a",
        "missing",
        IdempotencySendOutcome::Accepted(accepted_outcome()),
    );
    assert!(matches!(
        unknown.as_ref().unwrap_err().code(),
        IdempotencyErrorCode::Unknown
    ));

    let request = req(
        "agent.local/a",
        "key-1",
        "agent.work.compute",
        "sha256:abc",
        "corr-1",
    );
    broker.register_send(request, at(10)).unwrap();
    broker
        .commit_send(
            "agent.local/a",
            "key-1",
            IdempotencySendOutcome::Accepted(accepted_outcome()),
        )
        .unwrap();
    let twice = broker.commit_send(
        "agent.local/a",
        "key-1",
        IdempotencySendOutcome::Accepted(accepted_outcome()),
    );
    assert!(matches!(
        twice.as_ref().unwrap_err().code(),
        IdempotencyErrorCode::AlreadyCommitted
    ));
}

#[test]
fn empty_or_oversize_idempotency_key_is_rejected_with_validation_error() {
    let broker = Broker::new();
    let empty = IdempotencyRequest::new(
        "agent.local/a",
        "",
        "agent.work.compute",
        "sha256:abc",
        "send",
        "corr-1",
    );
    let oversize = IdempotencyRequest::new(
        "agent.local/a",
        "x".repeat(257),
        "agent.work.compute",
        "sha256:abc",
        "send",
        "corr-1",
    );

    let err1: IdempotencyError = broker.register_send(empty, at(10)).unwrap_err();
    let err2: IdempotencyError = broker.register_send(oversize, at(10)).unwrap_err();

    assert!(matches!(err1.code(), IdempotencyErrorCode::Validation));
    assert!(matches!(err2.code(), IdempotencyErrorCode::Validation));
}

#[test]
fn fixture_pins_idempotency_decision_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "idempotency|decision|first_attempt",
        "idempotency|decision|deduplicated",
        "idempotency|decision|conflict",
        "idempotency|decision|unknown",
        "idempotency|conflict|subject_mismatch",
        "idempotency|conflict|payload_fingerprint_mismatch",
        "idempotency|conflict|operation_kind_mismatch",
        "idempotency|error|E_IDEMPOTENCY_VALIDATION",
        "idempotency|error|E_IDEMPOTENCY_UNKNOWN",
        "idempotency|error|E_IDEMPOTENCY_ALREADY_COMMITTED",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
