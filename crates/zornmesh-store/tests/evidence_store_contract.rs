use std::path::{Path, PathBuf};

use zornmesh_core::{Envelope, REDACTION_MARKER};
use zornmesh_store::{
    EvidenceEnvelopeInput, EvidenceQuery, EvidenceStateTransitionInput, EvidenceStore,
    EvidenceStoreErrorCode, FileEvidenceStore,
};

fn temp_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "zornmesh-evidence-tests-{label}-{}-{}",
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

fn evidence_path(dir: &Path) -> PathBuf {
    dir.join("evidence.log")
}

fn envelope() -> Envelope {
    Envelope::with_metadata(
        "agent.local/source",
        "mesh.work.created",
        b"{\"password\":\"must-not-persist\"}".to_vec(),
        1_700_000_000_001,
        "corr-evidence-1",
        "application/json; token=must-not-persist",
    )
    .expect("valid envelope")
}

#[test]
fn accepted_envelope_audit_entry_and_trace_indexes_commit_atomically() {
    let dir = temp_dir("accepted");
    let path = evidence_path(&dir);
    let store = FileEvidenceStore::open_evidence(&path).expect("open evidence store");
    let input = EvidenceEnvelopeInput::new(envelope(), "msg-1", "trace-1", "accepted")
        .unwrap()
        .with_target("agent.local/target")
        .with_parent_message_id("parent-msg-0");

    let commit = store
        .persist_accepted_envelope(input)
        .expect("envelope evidence commits");

    assert_eq!(commit.envelope().daemon_sequence(), 1);
    assert_eq!(commit.envelope().message_id(), "msg-1");
    assert_eq!(commit.envelope().source_agent(), "agent.local/source");
    assert_eq!(commit.envelope().target_or_subject(), "agent.local/target");
    assert_eq!(commit.envelope().subject(), "mesh.work.created");
    assert_eq!(commit.envelope().timestamp_unix_ms(), 1_700_000_000_001);
    assert_eq!(commit.envelope().correlation_id(), "corr-evidence-1");
    assert_eq!(commit.envelope().trace_id(), "trace-1");
    assert_eq!(commit.envelope().parent_message_id(), Some("parent-msg-0"));
    assert_eq!(commit.envelope().delivery_state(), "accepted");
    assert_eq!(commit.envelope().payload_len(), 31);
    assert_eq!(commit.envelope().payload_content_type(), REDACTION_MARKER);

    let audit = commit.audit_entry();
    assert_eq!(audit.daemon_sequence(), 1);
    assert_eq!(audit.message_id(), "msg-1");
    assert_eq!(audit.previous_audit_hash(), "0");
    assert_ne!(audit.current_audit_hash(), "0");
    assert_eq!(audit.actor(), "agent.local/source");
    assert_eq!(audit.action(), "accepted_envelope");
    assert_eq!(audit.capability_or_subject(), "mesh.work.created");
    assert_eq!(audit.state_from(), None);
    assert_eq!(audit.state_to(), "accepted");
    assert!(audit.outcome_details().contains("durable processing"));

    let query = EvidenceQuery::new()
        .correlation_id("corr-evidence-1")
        .trace_id("trace-1")
        .agent_id("agent.local/source")
        .subject("mesh.work.created")
        .delivery_state("accepted")
        .time_window(1_700_000_000_000, 1_700_000_000_002);
    assert_eq!(
        store.query_envelopes(query),
        vec![commit.envelope().clone()]
    );

    let persisted = std::fs::read_to_string(&path).expect("evidence file readable");
    assert!(!persisted.contains("must-not-persist"));
    for index in [
        "idx_evidence_correlation_id",
        "idx_evidence_trace_id",
        "idx_evidence_agent_id",
        "idx_evidence_subject",
        "idx_evidence_delivery_state",
        "idx_evidence_timestamp",
    ] {
        assert!(
            store.index_names().contains(&index),
            "missing index {index}"
        );
    }
}

#[test]
fn audit_state_transitions_link_hashes_and_redact_outcome_details() {
    let dir = temp_dir("transition");
    let path = evidence_path(&dir);
    let store = FileEvidenceStore::open_evidence(&path).unwrap();
    let first = store
        .persist_accepted_envelope(
            EvidenceEnvelopeInput::new(envelope(), "msg-1", "trace-1", "accepted").unwrap(),
        )
        .unwrap();

    let transition = store
        .persist_state_transition(
            EvidenceStateTransitionInput::new(
                first.envelope().daemon_sequence(),
                "msg-1",
                "agent.local/authz",
                "authorization_decision",
                "mesh.work.created",
                "corr-evidence-1",
                "trace-1",
                "accepted",
                "authorized",
                "authorization allowed with password=must-not-persist",
            )
            .unwrap(),
        )
        .expect("state transition persists");

    assert_eq!(
        transition.previous_audit_hash(),
        first.audit_entry().current_audit_hash()
    );
    assert_ne!(
        transition.current_audit_hash(),
        first.audit_entry().current_audit_hash()
    );
    assert_eq!(transition.message_id(), "msg-1");
    assert_eq!(transition.daemon_sequence(), 1);
    assert_eq!(transition.state_from(), Some("accepted"));
    assert_eq!(transition.state_to(), "authorized");
    assert!(transition.outcome_details().contains(REDACTION_MARKER));
    assert!(!transition.outcome_details().contains("must-not-persist"));

    let updated = store
        .get_envelope("msg-1")
        .expect("record lookup succeeds")
        .expect("record exists");
    assert_eq!(updated.delivery_state(), "authorized");
}

#[test]
fn committed_records_recover_once_after_restart() {
    let dir = temp_dir("restart");
    let path = evidence_path(&dir);
    {
        let store = FileEvidenceStore::open_evidence(&path).unwrap();
        store
            .persist_accepted_envelope(
                EvidenceEnvelopeInput::new(envelope(), "msg-1", "trace-1", "accepted").unwrap(),
            )
            .unwrap();
    }

    let reopened = FileEvidenceStore::open_evidence(&path).unwrap();

    assert_eq!(reopened.next_daemon_sequence(), 2);
    assert_eq!(reopened.audit_entries().len(), 1);
    assert_eq!(reopened.query_envelopes(EvidenceQuery::new()).len(), 1);
}

#[test]
fn corrupt_future_schema_or_locked_migration_refuses_unsafe_writes() {
    let corrupt_dir = temp_dir("corrupt");
    let corrupt_path = evidence_path(&corrupt_dir);
    std::fs::write(&corrupt_path, "v1|tx|truncated\n").unwrap();
    let corrupt = FileEvidenceStore::open_evidence(&corrupt_path).unwrap_err();
    assert_eq!(corrupt.code(), EvidenceStoreErrorCode::Corrupt);

    let future_dir = temp_dir("future");
    let future_path = evidence_path(&future_dir);
    std::fs::write(&future_path, "v999|schema|evidence-store\n").unwrap();
    let future = FileEvidenceStore::open_evidence(&future_path).unwrap_err();
    assert_eq!(future.code(), EvidenceStoreErrorCode::FutureSchema);

    let locked_dir = temp_dir("locked");
    let locked_path = evidence_path(&locked_dir);
    std::fs::write(
        FileEvidenceStore::migration_lock_path(&locked_path),
        "other-migrator\n",
    )
    .unwrap();
    let locked = FileEvidenceStore::open_evidence(&locked_path).unwrap_err();
    assert_eq!(locked.code(), EvidenceStoreErrorCode::MigrationLocked);
}

#[test]
fn invalid_input_does_not_assign_sequence_or_persist_partial_evidence() {
    let dir = temp_dir("invalid");
    let path = evidence_path(&dir);
    let store = FileEvidenceStore::open_evidence(&path).unwrap();
    let err = EvidenceEnvelopeInput::new(envelope(), "msg-1", "", "accepted").unwrap_err();

    assert_eq!(err.code(), EvidenceStoreErrorCode::Validation);
    assert_eq!(store.next_daemon_sequence(), 1);
    assert!(store.query_envelopes(EvidenceQuery::new()).is_empty());
}

#[test]
fn fixture_pins_evidence_persistence_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "evidence|state|accepted",
        "evidence|state|authorized",
        "evidence|audit_action|accepted_envelope",
        "evidence|audit_action|authorization_decision",
        "evidence|index|idx_evidence_correlation_id",
        "evidence|index|idx_evidence_trace_id",
        "evidence|index|idx_evidence_agent_id",
        "evidence|index|idx_evidence_subject",
        "evidence|index|idx_evidence_delivery_state",
        "evidence|index|idx_evidence_timestamp",
        "evidence|error|E_EVIDENCE_VALIDATION",
        "evidence|error|E_EVIDENCE_CORRUPT",
        "evidence|error|E_EVIDENCE_FUTURE_SCHEMA",
        "evidence|error|E_EVIDENCE_MIGRATION_LOCKED",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
