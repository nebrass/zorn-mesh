use std::time::{Duration, SystemTime};

use zornmesh_cli::broker::{
    Broker, CancellationOutcome, ChunkSubmission, ChunkSubmissionOutcome, ReplySubmissionOutcome,
    RequestRegistration, RequestResolution, StreamErrorCode, StreamFinality, StreamRegistration,
    StreamState,
};
use zornmesh_cli::core::{CoordinationOutcomeKind, Envelope};

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn reply_envelope(correlation_id: &str) -> Envelope {
    Envelope::with_metadata(
        "agent.local/b",
        "agent.reply",
        b"{}".to_vec(),
        100,
        correlation_id.to_owned(),
        "application/json",
    )
    .expect("valid reply envelope")
}

#[test]
fn cancelling_in_flight_request_yields_terminal_cancelled_outcome() {
    let broker = Broker::new();
    let registration = RequestRegistration::new(
        "corr-cancel-1",
        "agent.local/a",
        "agent.local/b",
        "agent.work.compute",
        Duration::from_secs(30),
    );
    let handle = broker
        .register_request(registration, at(10))
        .expect("request registers");

    let outcome = broker
        .cancel_request("corr-cancel-1", at(11))
        .expect("cancel succeeds");

    match outcome {
        CancellationOutcome::Cancelled(coord) => {
            assert_eq!(coord.kind(), CoordinationOutcomeKind::Terminal);
            assert!(coord.terminal());
            assert_eq!(coord.code(), "CANCELLED");
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }

    let resolution = handle.recv_timeout(Duration::from_millis(50)).unwrap();
    match resolution {
        RequestResolution::Cancelled {
            correlation_id,
            outcome,
        } => {
            assert_eq!(correlation_id, "corr-cancel-1");
            assert_eq!(outcome.kind(), CoordinationOutcomeKind::Terminal);
        }
        other => panic!("expected Cancelled resolution, got {other:?}"),
    }
}

#[test]
fn reply_after_cancellation_is_recorded_as_late_event() {
    let broker = Broker::new();
    broker
        .register_request(
            RequestRegistration::new(
                "corr-cancel-2",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_secs(30),
            ),
            at(10),
        )
        .unwrap();

    broker.cancel_request("corr-cancel-2", at(11)).unwrap();

    let late = broker
        .submit_reply("corr-cancel-2", reply_envelope("corr-cancel-2"), at(12))
        .expect("late reply outcome returns");
    assert!(matches!(
        late,
        ReplySubmissionOutcome::DuplicateAfterTerminal
    ));
}

#[test]
fn reply_then_cancel_race_first_terminal_wins() {
    let broker = Broker::new();
    broker
        .register_request(
            RequestRegistration::new(
                "corr-race-1",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_secs(30),
            ),
            at(10),
        )
        .unwrap();

    // Reply lands first.
    broker
        .submit_reply("corr-race-1", reply_envelope("corr-race-1"), at(11))
        .unwrap();

    // Cancellation arrives second; reply already won.
    let outcome = broker.cancel_request("corr-race-1", at(12)).unwrap();
    assert!(matches!(outcome, CancellationOutcome::AlreadyComplete));
}

#[test]
fn cancelling_stream_marks_aborted_and_rejects_subsequent_chunks() {
    let broker = Broker::new();
    broker
        .open_stream(StreamRegistration::new(
            "stream-cancel-1",
            "corr-stream-1",
            "agent.local/a",
            "agent.local/b",
            4096,
            16384,
        ))
        .unwrap();

    let chunk = ChunkSubmission::new(
        "stream-cancel-1",
        0,
        vec![0u8; 1024],
        StreamFinality::Continue,
    );
    broker.submit_chunk(chunk).unwrap();

    let outcome = broker
        .cancel_stream_by_correlation("corr-stream-1")
        .expect("cancel stream succeeds");
    match outcome {
        CancellationOutcome::Cancelled(coord) => {
            assert_eq!(coord.kind(), CoordinationOutcomeKind::Terminal);
            assert!(coord.terminal());
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
    assert_eq!(
        broker.stream_state("stream-cancel-1"),
        Some(StreamState::Aborted)
    );

    // Further chunks rejected.
    let after = ChunkSubmission::new(
        "stream-cancel-1",
        1,
        vec![0u8; 1024],
        StreamFinality::Continue,
    );
    let err = broker.submit_chunk(after).unwrap_err();
    assert_eq!(err.code(), StreamErrorCode::StreamClosed);
}

#[test]
fn unknown_or_already_completed_correlation_returns_stable_typed_results() {
    let broker = Broker::new();

    // Unknown.
    let outcome = broker.cancel_request("missing", at(10)).unwrap();
    assert!(matches!(outcome, CancellationOutcome::NotFound));
    let outcome = broker.cancel_stream_by_correlation("missing").unwrap();
    assert!(matches!(outcome, CancellationOutcome::NotFound));

    // Already complete request.
    broker
        .register_request(
            RequestRegistration::new(
                "corr-done-1",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_secs(30),
            ),
            at(10),
        )
        .unwrap();
    broker
        .submit_reply("corr-done-1", reply_envelope("corr-done-1"), at(11))
        .unwrap();
    let outcome = broker.cancel_request("corr-done-1", at(12)).unwrap();
    assert!(matches!(outcome, CancellationOutcome::AlreadyComplete));

    // Already completed stream.
    broker
        .open_stream(StreamRegistration::new(
            "stream-done-1",
            "corr-stream-done",
            "agent.local/a",
            "agent.local/b",
            4096,
            16384,
        ))
        .unwrap();
    let final_chunk =
        ChunkSubmission::new("stream-done-1", 0, vec![0u8; 1024], StreamFinality::Final);
    let _ = broker.submit_chunk(final_chunk).unwrap();
    let outcome = broker
        .cancel_stream_by_correlation("corr-stream-done")
        .unwrap();
    assert!(matches!(outcome, CancellationOutcome::AlreadyComplete));
}

#[test]
fn timed_out_request_cannot_be_cancelled_and_reports_already_terminal() {
    let broker = Broker::new();
    broker
        .register_request(
            RequestRegistration::new(
                "corr-timeout-cancel",
                "agent.local/a",
                "agent.local/b",
                "agent.work.compute",
                Duration::from_millis(500),
            ),
            at(10),
        )
        .unwrap();
    broker.tick_request_timeouts(at(11));

    let outcome = broker
        .cancel_request("corr-timeout-cancel", at(12))
        .unwrap();
    match outcome {
        CancellationOutcome::AlreadyTimedOut | CancellationOutcome::AlreadyComplete => {}
        other => panic!("expected AlreadyTimedOut or AlreadyComplete, got {other:?}"),
    }
}

#[test]
fn submit_chunk_to_unrelated_stream_after_cancel_is_unaffected() {
    let broker = Broker::new();
    broker
        .open_stream(StreamRegistration::new(
            "stream-X", "corr-X", "a", "b", 4096, 16384,
        ))
        .unwrap();
    broker
        .open_stream(StreamRegistration::new(
            "stream-Y", "corr-Y", "a", "b", 4096, 16384,
        ))
        .unwrap();

    broker.cancel_stream_by_correlation("corr-X").unwrap();
    let on_y = ChunkSubmission::new("stream-Y", 0, vec![0u8; 1024], StreamFinality::Continue);
    let outcome = broker.submit_chunk(on_y).unwrap();
    assert!(matches!(outcome, ChunkSubmissionOutcome::Accepted { .. }));
}

#[test]
fn fixture_pins_cancellation_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "cancellation|outcome|cancelled",
        "cancellation|outcome|already_complete",
        "cancellation|outcome|already_timed_out",
        "cancellation|outcome|not_found",
        "cancellation|code|CANCELLED",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
