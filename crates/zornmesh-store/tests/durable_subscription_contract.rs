use std::path::PathBuf;

use zornmesh_store::{
    DurableSubscriptionScope, DurableSubscriptionStore, FileDurableStore, ResumeOutcome,
    SubscriptionStoreError, SubscriptionStoreErrorCode,
};

fn temp_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "zornmesh-store-tests-{label}-{}-{}",
        std::process::id(),
        rand_seed()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn rand_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

fn scope() -> DurableSubscriptionScope {
    DurableSubscriptionScope::new("agent.local/consumer", "mesh.work.>")
}

#[test]
fn create_subscription_persists_identity_and_scope_with_zero_position() {
    let dir = temp_dir("create");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).expect("open store");

    let state = store
        .create_subscription("sub-1", scope())
        .expect("create succeeds");
    assert_eq!(state.identity(), "sub-1");
    assert_eq!(state.scope().pattern(), "mesh.work.>");
    assert_eq!(state.last_acked_sequence(), 0);
    assert_eq!(state.retry_count(), 0);
}

#[test]
fn duplicate_identity_returns_conflict_without_overwriting_state() {
    let dir = temp_dir("conflict");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    store.create_subscription("sub-1", scope()).unwrap();
    store.record_ack("sub-1", 7).unwrap();

    let conflicting_scope =
        DurableSubscriptionScope::new("agent.local/consumer", "different.pattern");
    let err: SubscriptionStoreError = store
        .create_subscription("sub-1", conflicting_scope)
        .unwrap_err();
    assert_eq!(err.code(), SubscriptionStoreErrorCode::Conflict);

    // Original state intact.
    let resumed = store
        .resume_subscription("sub-1", scope(), 0)
        .expect("resume after conflict still works");
    let ResumeOutcome::Resumed { state } = resumed else {
        panic!("expected Resumed");
    };
    assert_eq!(state.last_acked_sequence(), 7);
}

#[test]
fn missing_identity_resume_returns_not_found_error() {
    let dir = temp_dir("missing");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    let err = store.resume_subscription("nope", scope(), 0).unwrap_err();
    assert_eq!(err.code(), SubscriptionStoreErrorCode::NotFound);
}

#[test]
fn ack_persists_position_and_survives_restart() {
    let dir = temp_dir("restart");
    let path = dir.join("durable.jsonl");
    {
        let store = FileDurableStore::open(&path).unwrap();
        store.create_subscription("sub-r", scope()).unwrap();
        store.record_ack("sub-r", 3).unwrap();
        store.record_ack("sub-r", 5).unwrap();
        store.record_retry("sub-r").unwrap();
    }

    // Simulated daemon restart: re-open the same path with a fresh store instance.
    let store = FileDurableStore::open(&path).unwrap();
    let resumed = store.resume_subscription("sub-r", scope(), 0).unwrap();
    let ResumeOutcome::Resumed { state } = resumed else {
        panic!("expected Resumed");
    };
    assert_eq!(state.last_acked_sequence(), 5);
    assert_eq!(state.retry_count(), 1);
}

#[test]
fn retention_gap_below_acked_position_is_reported_with_remediation() {
    let dir = temp_dir("retention");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    store.create_subscription("sub-g", scope()).unwrap();
    store.record_ack("sub-g", 5).unwrap();

    // Retention has trimmed below sub-g's last ack; trying to resume from
    // a position older than retention produces a structured gap result.
    let outcome = store.resume_subscription("sub-g", scope(), 100).unwrap();
    match outcome {
        ResumeOutcome::RetentionGap {
            requested_from,
            min_retained,
            remediation,
        } => {
            assert_eq!(requested_from, 100);
            assert!(min_retained <= 5);
            assert!(remediation.contains("retention"));
        }
        other => panic!("expected RetentionGap, got {other:?}"),
    }
}

#[test]
fn scope_mismatch_on_resume_returns_conflict() {
    let dir = temp_dir("scope-mismatch");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    store.create_subscription("sub-s", scope()).unwrap();

    let err = store
        .resume_subscription(
            "sub-s",
            DurableSubscriptionScope::new("agent.local/consumer", "other.pattern"),
            0,
        )
        .unwrap_err();
    assert_eq!(err.code(), SubscriptionStoreErrorCode::Conflict);
}

#[test]
fn empty_or_oversize_identity_is_rejected_with_validation_error() {
    let dir = temp_dir("validation");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    let empty = store.create_subscription("", scope()).unwrap_err();
    let oversize = store
        .create_subscription("x".repeat(257), scope())
        .unwrap_err();
    assert_eq!(empty.code(), SubscriptionStoreErrorCode::Validation);
    assert_eq!(oversize.code(), SubscriptionStoreErrorCode::Validation);
}

#[test]
fn fixture_pins_durable_subscription_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "durable_subscription|outcome|resumed",
        "durable_subscription|outcome|retention_gap",
        "durable_subscription|error|E_SUBSCRIPTION_VALIDATION",
        "durable_subscription|error|E_SUBSCRIPTION_NOT_FOUND",
        "durable_subscription|error|E_SUBSCRIPTION_CONFLICT",
        "durable_subscription|error|E_SUBSCRIPTION_IO",
        "durable_subscription|error|E_SUBSCRIPTION_CORRUPT",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}

#[test]
fn ack_must_be_monotonic_relative_to_recorded_position() {
    let dir = temp_dir("monotonic");
    let store = FileDurableStore::open(dir.join("durable.jsonl")).unwrap();
    store.create_subscription("sub-m", scope()).unwrap();
    store.record_ack("sub-m", 5).unwrap();
    let err = store.record_ack("sub-m", 3).unwrap_err();
    assert_eq!(err.code(), SubscriptionStoreErrorCode::Validation);
}
