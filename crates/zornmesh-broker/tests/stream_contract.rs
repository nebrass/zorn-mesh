use zornmesh_broker::{
    Broker, ChunkSubmission, ChunkSubmissionOutcome, StreamErrorCode, StreamFinality,
    StreamRegistration, StreamState, StreamTerminationReason,
};
use zornmesh_core::CoordinationOutcomeKind;

const KIB: usize = 1024;

fn registration(stream_id: &str, correlation_id: &str) -> StreamRegistration {
    StreamRegistration::new(
        stream_id,
        correlation_id,
        "agent.local/sender",
        "agent.local/receiver",
        4 * KIB,    // max chunk size
        16 * KIB,   // byte budget
    )
}

#[test]
fn happy_path_chunks_carry_sequence_and_terminal_chunk_completes_stream() {
    let broker = Broker::new();
    broker
        .open_stream(registration("stream-1", "corr-1"))
        .expect("stream registers");

    for seq in 0..3 {
        let chunk = ChunkSubmission::new("stream-1", seq, vec![0u8; KIB], StreamFinality::Continue);
        let outcome = broker.submit_chunk(chunk).expect("chunk submission completes");
        match outcome {
            ChunkSubmissionOutcome::Accepted {
                sequence,
                outstanding_bytes,
                ..
            } => {
                assert_eq!(sequence, seq);
                assert_eq!(outstanding_bytes, KIB * (seq as usize + 1));
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    let final_chunk = ChunkSubmission::new("stream-1", 3, vec![0u8; KIB], StreamFinality::Final);
    let outcome = broker
        .submit_chunk(final_chunk)
        .expect("final chunk submission completes");

    match outcome {
        ChunkSubmissionOutcome::Completed { send_outcome, .. } => {
            assert_eq!(send_outcome.kind(), CoordinationOutcomeKind::Acknowledged);
            assert!(send_outcome.terminal());
        }
        other => panic!("expected Completed, got {other:?}"),
    }
    assert_eq!(broker.stream_state("stream-1"), Some(StreamState::Completed));
}

#[test]
fn oversize_chunk_is_rejected_with_payload_limit_error_and_does_not_advance_stream() {
    let broker = Broker::new();
    broker.open_stream(registration("stream-2", "corr-2")).unwrap();

    let oversize = ChunkSubmission::new(
        "stream-2",
        0,
        vec![0u8; 4 * KIB + 1],
        StreamFinality::Continue,
    );
    let err = broker.submit_chunk(oversize).unwrap_err();
    assert_eq!(err.code(), StreamErrorCode::ChunkPayloadLimit);
    assert_eq!(broker.stream_state("stream-2"), Some(StreamState::Open));

    // Original sequence cursor unchanged: a valid first chunk still uses sequence 0.
    let recovered =
        ChunkSubmission::new("stream-2", 0, vec![0u8; KIB], StreamFinality::Continue);
    let outcome = broker.submit_chunk(recovered).unwrap();
    assert!(matches!(outcome, ChunkSubmissionOutcome::Accepted { sequence: 0, .. }));
}

#[test]
fn byte_budget_exhaustion_yields_typed_backpressure_outcome() {
    let broker = Broker::new();
    broker.open_stream(registration("stream-3", "corr-3")).unwrap();

    // 16 KiB budget; send four 4 KiB chunks then attempt a fifth.
    for seq in 0..4 {
        let chunk = ChunkSubmission::new("stream-3", seq, vec![0u8; 4 * KIB], StreamFinality::Continue);
        broker.submit_chunk(chunk).expect("chunk fits in budget");
    }

    let overflow = ChunkSubmission::new("stream-3", 4, vec![0u8; KIB], StreamFinality::Continue);
    let outcome = broker.submit_chunk(overflow).expect("budget exhausted outcome");
    match outcome {
        ChunkSubmissionOutcome::BudgetExhausted {
            outstanding_bytes,
            byte_budget,
            ..
        } => {
            assert_eq!(byte_budget, 16 * KIB);
            assert_eq!(outstanding_bytes, 16 * KIB);
        }
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
    assert_eq!(broker.stream_state("stream-3"), Some(StreamState::Open));

    // Receiver consumes some bytes → sender can resume.
    broker
        .acknowledge_consumed("stream-3", 4 * KIB)
        .expect("consume succeeds");
    let resumed =
        ChunkSubmission::new("stream-3", 4, vec![0u8; KIB], StreamFinality::Continue);
    let resumed_outcome = broker.submit_chunk(resumed).unwrap();
    assert!(matches!(resumed_outcome, ChunkSubmissionOutcome::Accepted { sequence: 4, .. }));
}

#[test]
fn out_of_order_sequence_is_detected_as_gap_and_does_not_corrupt_stream() {
    let broker = Broker::new();
    broker.open_stream(registration("stream-4", "corr-4")).unwrap();

    let first = ChunkSubmission::new("stream-4", 0, vec![0u8; KIB], StreamFinality::Continue);
    broker.submit_chunk(first).unwrap();

    // Skipping sequence 1.
    let gap = ChunkSubmission::new("stream-4", 2, vec![0u8; KIB], StreamFinality::Continue);
    let outcome = broker.submit_chunk(gap).expect("gap outcome");
    match outcome {
        ChunkSubmissionOutcome::SequenceGap {
            expected,
            received,
        } => {
            assert_eq!(expected, 1);
            assert_eq!(received, 2);
        }
        other => panic!("expected SequenceGap, got {other:?}"),
    }
    assert_eq!(broker.stream_state("stream-4"), Some(StreamState::Open));
}

#[test]
fn submitting_to_unknown_or_closed_stream_returns_typed_error() {
    let broker = Broker::new();
    let unknown =
        ChunkSubmission::new("nope", 0, vec![0u8; KIB], StreamFinality::Continue);
    let err = broker.submit_chunk(unknown).unwrap_err();
    assert_eq!(err.code(), StreamErrorCode::StreamUnknown);

    broker.open_stream(registration("stream-5", "corr-5")).unwrap();
    let final_chunk =
        ChunkSubmission::new("stream-5", 0, vec![0u8; KIB], StreamFinality::Final);
    broker.submit_chunk(final_chunk).unwrap();

    // Stream completed; cannot submit more.
    let after_close =
        ChunkSubmission::new("stream-5", 1, vec![0u8; KIB], StreamFinality::Continue);
    let err = broker.submit_chunk(after_close).unwrap_err();
    assert_eq!(err.code(), StreamErrorCode::StreamClosed);
}

#[test]
fn abort_stream_marks_terminal_failed_state_observable_to_both_sides() {
    let broker = Broker::new();
    broker.open_stream(registration("stream-6", "corr-6")).unwrap();
    let chunk =
        ChunkSubmission::new("stream-6", 0, vec![0u8; KIB], StreamFinality::Continue);
    broker.submit_chunk(chunk).unwrap();

    let outcome = broker
        .abort_stream("stream-6", StreamTerminationReason::ReceiverFailure)
        .expect("abort succeeds");
    assert_eq!(outcome.kind(), CoordinationOutcomeKind::Failed);
    assert!(outcome.terminal());
    assert_eq!(broker.stream_state("stream-6"), Some(StreamState::Aborted));

    // Subsequent chunks are rejected as closed.
    let after_abort =
        ChunkSubmission::new("stream-6", 1, vec![0u8; KIB], StreamFinality::Continue);
    let err = broker.submit_chunk(after_abort).unwrap_err();
    assert_eq!(err.code(), StreamErrorCode::StreamClosed);
}

#[test]
fn fixture_pins_stream_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "stream|state|open",
        "stream|state|completed",
        "stream|state|aborted",
        "stream|finality|continue",
        "stream|finality|final",
        "stream|outcome|accepted",
        "stream|outcome|completed",
        "stream|outcome|budget_exhausted",
        "stream|outcome|sequence_gap",
        "stream|error|E_STREAM_UNKNOWN",
        "stream|error|E_STREAM_CLOSED",
        "stream|error|E_CHUNK_PAYLOAD_LIMIT",
        "stream|error|E_STREAM_VALIDATION",
        "stream|termination|sender_cancelled",
        "stream|termination|receiver_failure",
        "stream|termination|daemon_disconnect",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
