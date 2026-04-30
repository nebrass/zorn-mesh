//! Contract tests for the v0.2 debate substrate.
//!
//! Covers:
//! - subject taxonomy is stable (golden strings the v0.3+ code MUST not change)
//! - envelope encode/decode round-trips
//! - orchestrator end-to-end with a synthetic worker thread that publishes a
//!   critique envelope (no platform CLI needed)
//! - quorum + timeout boundary behavior
//! - dissent-point preservation in the synthesized consensus

use std::{
    sync::mpsc::channel,
    thread,
    time::{Duration, Instant},
};

use zornmesh_cli::broker::{Broker, DeliveryAttempt, PeerCredentials, SocketTrustPolicy};
use zornmesh_cli::core::Envelope;
use zornmesh_cli::debate::{
    ConsensusEnvelope, CritiqueEnvelope, DEBATE_SCHEMA_VERSION, DebateOptions, DebateOrchestrator,
    DissentRecord, EnvelopeDecodeError, PlanEnvelope, WORKER_PLAN_SUBSCRIPTION, subject_consensus,
    subject_critique, subject_critique_pattern, subject_plan,
};

/// Pinned by golden taxonomy contract: any change to these strings is a
/// wire-format break, must bump v0.3 schema_version.
#[test]
fn subject_taxonomy_is_pinned() {
    assert_eq!(subject_plan("X"), "debate.X.plan");
    assert_eq!(subject_critique("X", "claude"), "debate.X.critique.claude");
    assert_eq!(subject_critique_pattern("X"), "debate.X.critique.>");
    assert_eq!(subject_consensus("X"), "debate.X.consensus");
    assert_eq!(WORKER_PLAN_SUBSCRIPTION, "debate.*.plan");
    assert_eq!(DEBATE_SCHEMA_VERSION, "zornmesh.debate.v1");
}

#[test]
fn plan_envelope_round_trips() {
    let plan = PlanEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: "deb-1".to_owned(),
        originator: "agent.driver.cli".to_owned(),
        plan: "implement payment retry logic".to_owned(),
        repo: Some("/tmp/repo".to_owned()),
        deadline_unix_ms: 1_700_000_000_000,
        max_tokens: Some(8192),
    };
    let bytes = plan.to_bytes();
    let decoded = PlanEnvelope::from_bytes(&bytes).expect("decodes");
    assert_eq!(plan, decoded);
}

#[test]
fn critique_envelope_round_trips_with_dissent_points() {
    let critique = CritiqueEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: "deb-1".to_owned(),
        agent: "agent.worker.gemini".to_owned(),
        critique: "Plan ignores idempotency.".to_owned(),
        agreement_score: 70,
        dissent_points: vec![
            "missing_idempotency_key".to_owned(),
            "no_retry_budget".to_owned(),
        ],
        cost_tokens: Some(420),
    };
    let bytes = critique.to_bytes();
    let decoded = CritiqueEnvelope::from_bytes(&bytes).expect("decodes");
    assert_eq!(critique, decoded);
}

#[test]
fn critique_envelope_rejects_non_critique_kind() {
    let plan = PlanEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: "deb-1".to_owned(),
        originator: "x".to_owned(),
        plan: "p".to_owned(),
        repo: None,
        deadline_unix_ms: 0,
        max_tokens: None,
    };
    let bytes = plan.to_bytes();
    let result = CritiqueEnvelope::from_bytes(&bytes);
    let err: EnvelopeDecodeError = result.unwrap_err();
    assert!(err.message.contains("not 'critique'"), "got: {}", err.message);
}

#[test]
fn orchestrator_aggregates_critiques_into_consensus() {
    let broker = Broker::new();
    let credentials = PeerCredentials::new(0, 0, std::process::id());
    let trust_policy = SocketTrustPolicy::new(0, 0, 0o600);

    // Spawn a "worker thread" that subscribes to plan subjects and replies.
    let synthetic_worker = spawn_synthetic_worker(broker.clone(), "agent.worker.synthetic");

    let orchestrator = DebateOrchestrator::new(&broker, credentials, trust_policy);
    let outcome = orchestrator
        .run(
            DebateOptions::new("agent.driver.test", "Refactor the payment module")
                .with_timeout(Duration::from_secs(2))
                .with_quorum(1),
        )
        .expect("debate succeeds");

    assert_eq!(outcome.critiques.len(), 1);
    assert_eq!(outcome.critiques[0].agent, "agent.worker.synthetic");
    assert!(outcome
        .consensus
        .consensus
        .contains("Refactor the payment module"));
    assert!(outcome
        .consensus
        .consensus
        .contains("agent.worker.synthetic"));

    // Make sure the synthetic worker thread is shut down.
    synthetic_worker.shutdown();
}

#[test]
fn orchestrator_returns_empty_consensus_on_timeout() {
    let broker = Broker::new();
    let credentials = PeerCredentials::new(0, 0, std::process::id());
    let trust_policy = SocketTrustPolicy::new(0, 0, 0o600);
    let orchestrator = DebateOrchestrator::new(&broker, credentials, trust_policy);

    let started = Instant::now();
    let outcome = orchestrator
        .run(
            DebateOptions::new("agent.driver.test", "no-workers plan")
                .with_timeout(Duration::from_millis(200))
                .with_quorum(1),
        )
        .expect("debate must not error on timeout, just return empty");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(150),
        "should have waited for the timeout, elapsed={elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(800),
        "should not exceed the timeout meaningfully, elapsed={elapsed:?}"
    );
    assert!(outcome.critiques.is_empty());
    assert!(outcome.consensus.consensus.contains("no critiques received"));
}

#[test]
fn dissent_records_are_preserved_in_consensus() {
    let broker = Broker::new();
    let credentials = PeerCredentials::new(0, 0, std::process::id());
    let trust_policy = SocketTrustPolicy::new(0, 0, 0o600);

    let dissenting_worker = spawn_dissenting_worker(broker.clone(), "agent.worker.dissent");

    let orchestrator = DebateOrchestrator::new(&broker, credentials, trust_policy);
    let outcome = orchestrator
        .run(
            DebateOptions::new("agent.driver.test", "broken plan with security holes")
                .with_timeout(Duration::from_secs(2))
                .with_quorum(1),
        )
        .expect("debate succeeds");

    assert_eq!(outcome.consensus.dissent.len(), 1);
    let dissent: &DissentRecord = &outcome.consensus.dissent[0];
    assert_eq!(dissent.agent, "agent.worker.dissent");
    assert_eq!(dissent.points, vec!["sql_injection_risk".to_owned()]);
    assert!(outcome.consensus.consensus.contains("DISSENT POINTS"));
    assert!(outcome.consensus.consensus.contains("sql_injection_risk"));

    dissenting_worker.shutdown();
}

#[test]
fn orchestrator_rejects_empty_plan() {
    let broker = Broker::new();
    let credentials = PeerCredentials::new(0, 0, std::process::id());
    let trust_policy = SocketTrustPolicy::new(0, 0, 0o600);
    let orchestrator = DebateOrchestrator::new(&broker, credentials, trust_policy);
    let result = orchestrator.run(DebateOptions::new("a", "   "));
    assert!(matches!(
        result.as_ref().unwrap_err().code(),
        "E_DEBATE_INVALID_PLAN"
    ));
}

#[test]
fn consensus_envelope_serialization_is_stable() {
    let consensus = ConsensusEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: "deb-1".to_owned(),
        consensus: "synthesized text".to_owned(),
        dissent: vec![DissentRecord {
            agent: "agent.worker.gemini".to_owned(),
            points: vec!["concurrency_risk".to_owned()],
        }],
        participants: vec!["agent.worker.gemini".to_owned()],
        timed_out: vec!["agent.worker.copilot".to_owned()],
        total_cost_tokens: 1024,
        round: 1,
    };
    let bytes = consensus.to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["kind"], "consensus");
    assert_eq!(v["schema_version"], DEBATE_SCHEMA_VERSION);
    assert_eq!(v["debate_id"], "deb-1");
    assert_eq!(v["dissent"][0]["agent"], "agent.worker.gemini");
    assert_eq!(v["timed_out"][0], "agent.worker.copilot");
}

// ---- synthetic worker helpers (no platform CLI needed) ----

struct SyntheticWorker {
    handle: Option<thread::JoinHandle<()>>,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SyntheticWorker {
    fn shutdown(mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            // Best-effort: the thread will exit on its next recv_timeout tick.
            let _ = h.join();
        }
    }
}

fn spawn_synthetic_worker(broker: Broker, agent_id: &str) -> SyntheticWorker {
    let agent_id = agent_id.to_owned();
    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag_clone = std::sync::Arc::clone(&stop_flag);
    let (subscribed_tx, subscribed_rx) = std::sync::mpsc::sync_channel::<()>(0);
    let handle = thread::spawn(move || {
        let (tx, rx) = channel::<DeliveryAttempt>();
        let _sub = broker
            .subscribe(WORKER_PLAN_SUBSCRIPTION, tx)
            .expect("worker subscribes");
        // Signal the parent that the subscription is registered before any
        // race with the orchestrator's publish.
        let _ = subscribed_tx.send(());
        loop {
            if stop_flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(delivery) => {
                    let plan = PlanEnvelope::from_bytes(delivery.envelope().payload())
                        .expect("plan decodes");
                    if plan.originator == agent_id {
                        continue;
                    }
                    let critique = CritiqueEnvelope {
                        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
                        debate_id: plan.debate_id.clone(),
                        agent: agent_id.clone(),
                        critique: format!("synthetic critique of: {}", plan.plan),
                        agreement_score: 80,
                        dissent_points: vec![],
                        cost_tokens: Some(123),
                    };
                    let env = Envelope::new(
                        agent_id.clone(),
                        subject_critique(&plan.debate_id, "synthetic"),
                        critique.to_bytes(),
                    )
                    .expect("envelope");
                    let _ = broker.publish(env);
                }
                Err(_) => continue,
            }
        }
    });
    subscribed_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("synthetic worker subscribed in time");
    SyntheticWorker {
        handle: Some(handle),
        stop_flag,
    }
}

fn spawn_dissenting_worker(broker: Broker, agent_id: &str) -> SyntheticWorker {
    let agent_id = agent_id.to_owned();
    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag_clone = std::sync::Arc::clone(&stop_flag);
    let (subscribed_tx, subscribed_rx) = std::sync::mpsc::sync_channel::<()>(0);
    let handle = thread::spawn(move || {
        let (tx, rx) = channel::<DeliveryAttempt>();
        let _sub = broker
            .subscribe(WORKER_PLAN_SUBSCRIPTION, tx)
            .expect("worker subscribes");
        let _ = subscribed_tx.send(());
        loop {
            if stop_flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(delivery) => {
                    let plan = PlanEnvelope::from_bytes(delivery.envelope().payload())
                        .expect("plan decodes");
                    if plan.originator == agent_id {
                        continue;
                    }
                    let critique = CritiqueEnvelope {
                        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
                        debate_id: plan.debate_id.clone(),
                        agent: agent_id.clone(),
                        critique: format!("dissenting critique of: {}", plan.plan),
                        agreement_score: 20,
                        dissent_points: vec!["sql_injection_risk".to_owned()],
                        cost_tokens: Some(99),
                    };
                    let env = Envelope::new(
                        agent_id.clone(),
                        subject_critique(&plan.debate_id, "dissent"),
                        critique.to_bytes(),
                    )
                    .expect("envelope");
                    let _ = broker.publish(env);
                }
                Err(_) => continue,
            }
        }
    });
    subscribed_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("dissenting worker subscribed in time");
    SyntheticWorker {
        handle: Some(handle),
        stop_flag,
    }
}
