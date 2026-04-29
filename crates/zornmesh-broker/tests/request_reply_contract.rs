use std::time::{Duration, SystemTime};

use zornmesh_broker::{
    Broker, LateRequestKind, ReplySubmissionOutcome, RequestRegistration, RequestResolution,
};
use zornmesh_core::{CoordinationOutcomeKind, CoordinationStage, Envelope, NackReasonCategory};

const REPLY_CONTENT: &[u8] = b"{\"ok\":true}";

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn reply_envelope(correlation_id: &str, source: &str) -> Envelope {
    Envelope::with_metadata(
        source,
        "agent.reply",
        REPLY_CONTENT.to_vec(),
        100,
        correlation_id.to_owned(),
        "application/json",
    )
    .expect("valid reply envelope")
}

#[test]
fn happy_path_delivers_one_correlated_reply_before_timeout() {
    let broker = Broker::new();
    let registration = RequestRegistration::new(
        "corr-happy-1",
        "agent.local/a",
        "agent.local/b",
        "agent.work.compute",
        Duration::from_secs(5),
    );

    let handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let submission = broker
        .submit_reply(
            "corr-happy-1",
            reply_envelope("corr-happy-1", "agent.local/b"),
            at(11),
        )
        .expect("reply submits");

    assert!(matches!(submission, ReplySubmissionOutcome::Accepted));

    let resolution = handle
        .recv_timeout(Duration::from_millis(50))
        .expect("requester receives reply");

    match resolution {
        RequestResolution::Replied { envelope, attempt } => {
            assert_eq!(envelope.correlation_id(), "corr-happy-1");
            assert_eq!(envelope.source_agent(), "agent.local/b");
            assert_eq!(attempt, 1);
        }
        other => panic!("expected Replied, got {other:?}"),
    }
}

#[test]
fn timeout_yields_typed_timed_out_resolution_and_seals_request() {
    let broker = Broker::new();
    let registration = RequestRegistration::new(
        "corr-timeout-1",
        "agent.local/a",
        "agent.local/b",
        "agent.work.slow",
        Duration::from_millis(500),
    );

    let handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let expired = broker.tick_request_timeouts(at(11));
    assert_eq!(expired.len(), 1, "one request should have expired");

    let resolution = handle
        .recv_timeout(Duration::from_millis(50))
        .expect("requester receives timeout");

    match resolution {
        RequestResolution::TimedOut { outcome, .. } => {
            assert_eq!(outcome.kind(), CoordinationOutcomeKind::TimedOut);
            assert_eq!(outcome.stage(), CoordinationStage::Transport);
            assert!(outcome.terminal());
        }
        other => panic!("expected TimedOut, got {other:?}"),
    }

    // Late reply must not deliver as success.
    let late = broker
        .submit_reply(
            "corr-timeout-1",
            reply_envelope("corr-timeout-1", "agent.local/b"),
            at(12),
        )
        .expect("late reply submission returns outcome");
    assert!(matches!(late, ReplySubmissionOutcome::LateAfterTimeout));
}

#[test]
fn rejected_reply_yields_typed_failure_with_safe_details_and_retryable_flag() {
    let broker = Broker::new();
    let registration = RequestRegistration::new(
        "corr-reject-1",
        "agent.local/a",
        "agent.local/b",
        "agent.work.unauthorized",
        Duration::from_secs(5),
    );

    let handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let submission = broker
        .submit_request_failure(
            "corr-reject-1",
            NackReasonCategory::Authorization,
            "caller is not authorized for agent.work.unauthorized",
            false,
            at(11),
        )
        .expect("rejection submits");
    assert!(matches!(submission, ReplySubmissionOutcome::Accepted));

    let resolution = handle
        .recv_timeout(Duration::from_millis(50))
        .expect("requester receives rejection");

    match resolution {
        RequestResolution::Rejected { outcome, reason } => {
            assert_eq!(outcome.kind(), CoordinationOutcomeKind::Rejected);
            assert_eq!(outcome.stage(), CoordinationStage::Delivery);
            assert!(outcome.terminal());
            assert!(!outcome.retryable());
            assert_eq!(reason, NackReasonCategory::Authorization);
            assert!(outcome.message().contains("not authorized"));
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[test]
fn first_terminal_reply_wins_and_subsequent_replies_are_recorded_as_duplicates() {
    let broker = Broker::new();
    let registration = RequestRegistration::new(
        "corr-dup-1",
        "agent.local/a",
        "agent.local/b",
        "agent.work.compute",
        Duration::from_secs(5),
    );
    let handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let first = broker
        .submit_reply(
            "corr-dup-1",
            reply_envelope("corr-dup-1", "agent.local/b"),
            at(11),
        )
        .expect("first reply accepted");
    assert!(matches!(first, ReplySubmissionOutcome::Accepted));

    let second = broker
        .submit_reply(
            "corr-dup-1",
            reply_envelope("corr-dup-1", "agent.local/b"),
            at(12),
        )
        .expect("second reply outcome");
    assert!(matches!(
        second,
        ReplySubmissionOutcome::DuplicateAfterTerminal
    ));

    // Only one resolution reaches the requester.
    let _ = handle
        .recv_timeout(Duration::from_millis(50))
        .expect("first resolution arrives");
    assert!(handle.recv_timeout(Duration::from_millis(20)).is_err());

    // Late events are visible as audit records.
    let late_events = broker.late_request_events();
    assert_eq!(late_events.len(), 1);
    assert_eq!(late_events[0].correlation_id(), "corr-dup-1");
}

#[test]
fn concurrent_requests_match_replies_by_correlation_id_in_reverse_order() {
    let broker = Broker::new();
    let h1 = broker
        .register_request(
            RequestRegistration::new(
                "corr-A",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_secs(5),
            ),
            at(10),
        )
        .expect("first request registers");
    let h2 = broker
        .register_request(
            RequestRegistration::new(
                "corr-B",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_secs(5),
            ),
            at(10),
        )
        .expect("second request registers");

    // Replies arrive in reverse order.
    broker
        .submit_reply("corr-B", reply_envelope("corr-B", "agent.local/b"), at(11))
        .expect("B reply accepted");
    broker
        .submit_reply("corr-A", reply_envelope("corr-A", "agent.local/b"), at(12))
        .expect("A reply accepted");

    match h1.recv_timeout(Duration::from_millis(50)) {
        Ok(RequestResolution::Replied { envelope, .. }) => {
            assert_eq!(envelope.correlation_id(), "corr-A");
        }
        other => panic!("expected Replied for corr-A, got {other:?}"),
    }
    match h2.recv_timeout(Duration::from_millis(50)) {
        Ok(RequestResolution::Replied { envelope, .. }) => {
            assert_eq!(envelope.correlation_id(), "corr-B");
        }
        other => panic!("expected Replied for corr-B, got {other:?}"),
    }
}

#[test]
fn unknown_correlation_id_reply_is_recorded_as_orphan_and_not_delivered() {
    let broker = Broker::new();
    let outcome = broker
        .submit_reply(
            "corr-orphan-1",
            reply_envelope("corr-orphan-1", "agent.local/b"),
            at(10),
        )
        .expect("orphan reply outcome returns");
    assert!(matches!(
        outcome,
        ReplySubmissionOutcome::UnknownCorrelation
    ));

    let late = broker.late_request_events();
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].correlation_id(), "corr-orphan-1");
    assert_eq!(late[0].kind(), LateRequestKind::UnknownCorrelation);
}

#[test]
fn fixture_pins_request_reply_resolution_and_late_event_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "request_reply|resolution|replied",
        "request_reply|resolution|rejected",
        "request_reply|resolution|timed_out",
        "request_reply|late_event|duplicate_after_terminal",
        "request_reply|late_event|late_after_timeout",
        "request_reply|late_event|unknown_correlation",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
