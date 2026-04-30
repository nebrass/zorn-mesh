//! CLI-side runners that connect to the **real per-user daemon** via the SDK
//! so workers and orchestrators in different terminals share state.
//!
//! Why this exists separately from `WorkerDaemon` and `DebateOrchestrator`:
//! the in-process types take a `&Broker` and are perfect for unit tests and
//! single-process embedders, but the actual end-user use case ("worker in
//! terminal A, orchestrator in terminal B") requires a shared broker, which
//! is precisely what the daemon's Unix socket provides. The `Mesh::connect`
//! path autospawns a daemon if none is reachable; multiple SDK consumers
//! share that single daemon.
//!
//! v0.2.1 hotfix: v0.2.0 wired the CLI commands against an in-process Broker
//! per process, which silently broke the very use case the substrate was
//! built for.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::core::Envelope;
use crate::sdk::{Mesh, SdkError, SdkErrorCode, SendResult, SendStatus};

use super::{
    ConsensusEnvelope, CritiqueEnvelope, DEBATE_SCHEMA_VERSION, DebateError, DebateOptions,
    DebateOutcome, DissentRecord, PlanEnvelope, Platform, PlatformAdapter, WORKER_PLAN_SUBSCRIPTION,
    platforms, subject_consensus, subject_critique, subject_critique_pattern, subject_plan,
    worker::build_critique_prompt,
};

// ---- shared error helpers ----

fn debate_err_from_sdk(prefix: &str, error: SdkError) -> DebateError {
    DebateError::BrokerFailure(format!("{prefix}: {} {}", error.code().as_str(), error))
}

fn debate_err_from_send(result: SendResult) -> DebateError {
    DebateError::PublishFailure(format!(
        "publish {:?}: {} {}",
        result.status(),
        result.code(),
        result.message(),
    ))
}

fn send_accepted(result: &SendResult) -> bool {
    matches!(result.status(), SendStatus::Accepted)
}

// ---- orchestrator (driver-side) ----

/// Runs a single debate end-to-end against the real per-user daemon.
///
/// This is what `zornmesh debate run` invokes. The orchestrator connects to
/// the daemon via the SDK, subscribes to the critique pattern BEFORE
/// publishing the plan (avoiding the publish-vs-subscribe race), waits for
/// quorum or timeout, then synthesizes a consensus that explicitly preserves
/// dissent points.
pub fn run_debate_via_daemon(options: DebateOptions) -> Result<DebateOutcome, DebateError> {
    if options.plan.trim().is_empty() {
        return Err(DebateError::InvalidPlan(
            "plan must be a non-empty string".to_owned(),
        ));
    }

    let mesh = Mesh::connect().map_err(|err| debate_err_from_sdk("connect", err))?;

    let debate_id = generate_debate_id();
    let plan_subject = subject_plan(&debate_id);
    let critique_pattern = subject_critique_pattern(&debate_id);
    let consensus_subject = subject_consensus(&debate_id);
    let deadline_unix_ms = current_unix_ms() + options.timeout.as_millis() as u64;

    // Subscribe BEFORE publishing the plan.
    let mut subscription = mesh
        .subscribe(critique_pattern)
        .map_err(|err| debate_err_from_sdk("subscribe critiques", err))?;

    let plan = PlanEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: debate_id.clone(),
        originator: options.originator.clone(),
        plan: options.plan.clone(),
        repo: options.repo.clone(),
        deadline_unix_ms,
        max_tokens: options.max_tokens,
    };
    let plan_envelope = Envelope::new(
        options.originator.clone(),
        plan_subject.clone(),
        plan.to_bytes(),
    )
    .map_err(|err| DebateError::PublishFailure(format!("plan envelope: {err:?}")))?;
    let send_result = mesh.publish(&plan_envelope);
    if !send_accepted(&send_result) {
        return Err(debate_err_from_send(send_result));
    }

    // Drain critiques until quorum or timeout.
    let critiques = collect_critiques(&mut subscription, options.timeout, options.quorum, &debate_id);

    // Synthesize consensus and best-effort publish.
    let consensus = synthesize(&debate_id, &options.plan, &critiques, 1);
    if let Ok(env) = Envelope::new(
        options.originator.clone(),
        consensus_subject,
        consensus.to_bytes(),
    ) {
        let _ = mesh.publish(&env); // best-effort; outcome is already returned to caller
    }

    Ok(DebateOutcome {
        debate_id,
        plan: options.plan,
        critiques,
        consensus,
    })
}

fn collect_critiques(
    subscription: &mut crate::sdk::Subscription,
    timeout: Duration,
    quorum: u32,
    debate_id: &str,
) -> Vec<CritiqueEnvelope> {
    let deadline = Instant::now() + timeout;
    let mut critiques: Vec<CritiqueEnvelope> = Vec::new();
    while critiques.len() < quorum as usize {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match subscription.recv_delivery(remaining) {
            Ok(Some(delivery)) => {
                if let Ok(critique) = CritiqueEnvelope::from_bytes(delivery.envelope().payload()) {
                    if critique.debate_id == debate_id
                        && !critiques.iter().any(|c| c.agent == critique.agent)
                    {
                        critiques.push(critique);
                    }
                }
            }
            Ok(None) => break, // timeout from the SDK
            Err(_) => break,   // disconnect or protocol error -- stop collecting
        }
    }
    critiques
}

fn synthesize(
    debate_id: &str,
    original_plan: &str,
    critiques: &[CritiqueEnvelope],
    round: u32,
) -> ConsensusEnvelope {
    let participants: Vec<String> = critiques.iter().map(|c| c.agent.clone()).collect();
    let dissent: Vec<DissentRecord> = critiques
        .iter()
        .filter(|c| !c.dissent_points.is_empty())
        .map(|c| DissentRecord {
            agent: c.agent.clone(),
            points: c.dissent_points.clone(),
        })
        .collect();
    let total_cost_tokens: u64 = critiques.iter().filter_map(|c| c.cost_tokens).sum();

    let mut consensus = String::new();
    consensus.push_str("ORIGINAL PLAN:\n");
    consensus.push_str(original_plan);
    consensus.push_str("\n\nCRITIQUES:\n");
    if critiques.is_empty() {
        consensus.push_str("(no critiques received before timeout)\n");
    } else {
        for c in critiques {
            consensus.push_str(&format!(
                "[{} | agreement={}] {}\n",
                c.agent, c.agreement_score, c.critique
            ));
        }
    }
    if !dissent.is_empty() {
        consensus.push_str("\nDISSENT POINTS:\n");
        for d in &dissent {
            consensus.push_str(&format!("  {}: {}\n", d.agent, d.points.join("; ")));
        }
    }

    ConsensusEnvelope {
        schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
        debate_id: debate_id.to_owned(),
        consensus,
        dissent,
        participants,
        timed_out: Vec::new(),
        total_cost_tokens,
        round,
    }
}

// ---- worker (responder-side) ----

/// Long-lived worker: connects to the daemon, subscribes to plans, and on
/// each delivery shells out to the platform CLI in non-interactive mode.
///
/// `stop_after` is a CI/test knob -- pass `None` for production daemons that
/// should run forever.
pub fn run_worker_via_daemon(
    platform: Platform,
    invocation_timeout: Duration,
    stop_after: Option<Duration>,
) -> Result<(), DebateError> {
    let mesh = Mesh::connect().map_err(|err| debate_err_from_sdk("worker connect", err))?;
    let agent_id = format!("agent.worker.{}", platform.name());
    let adapter = platforms::adapter_for(platform);

    eprintln!(
        "zornmesh worker: platform={} agent_id={} subscription={}",
        platform.name(),
        agent_id,
        WORKER_PLAN_SUBSCRIPTION,
    );

    let mut subscription = mesh
        .subscribe(WORKER_PLAN_SUBSCRIPTION)
        .map_err(|err| debate_err_from_sdk("worker subscribe", err))?;

    let started = Instant::now();
    loop {
        if let Some(stop) = stop_after {
            if started.elapsed() >= stop {
                return Ok(());
            }
        }
        // Use a finite recv tick so the SIGTERM path remains responsive --
        // each tick is cheap on the daemon socket.
        let recv_timeout = Duration::from_secs(60);
        match subscription.recv_delivery(recv_timeout) {
            Ok(Some(delivery)) => {
                handle_plan_delivery(
                    &mesh,
                    &agent_id,
                    adapter.as_ref(),
                    &delivery,
                    invocation_timeout,
                );
            }
            Ok(None) => continue, // tick timeout, keep waiting
            Err(error) if error.code() == SdkErrorCode::DaemonUnreachable => {
                return Err(debate_err_from_sdk("worker daemon", error));
            }
            Err(error) => {
                eprintln!(
                    "zornmesh worker: transient delivery error {}: {}",
                    error.code().as_str(),
                    error
                );
                continue;
            }
        }
    }
}

fn handle_plan_delivery(
    mesh: &Mesh,
    agent_id: &str,
    adapter: &dyn PlatformAdapter,
    delivery: &crate::sdk::Delivery,
    invocation_timeout: Duration,
) {
    let plan = match PlanEnvelope::from_bytes(delivery.envelope().payload()) {
        Ok(plan) => plan,
        Err(_) => return,
    };
    if plan.originator == agent_id {
        // Don't critique your own plans.
        return;
    }

    let prompt = build_critique_prompt(&plan);
    let invocation_result = adapter.invoke(&prompt, plan.repo.as_deref(), invocation_timeout);

    let critique = match invocation_result {
        Ok(invocation) => {
            let body = if invocation.stdout.trim().is_empty() {
                format!(
                    "(empty response from {} CLI; exit_status={:?})",
                    adapter.platform().name(),
                    invocation.exit_status,
                )
            } else {
                invocation.stdout.trim().to_owned()
            };
            CritiqueEnvelope {
                schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
                debate_id: plan.debate_id.clone(),
                agent: agent_id.to_owned(),
                critique: body,
                agreement_score: 50,
                dissent_points: Vec::new(),
                cost_tokens: None,
            }
        }
        Err(err) => CritiqueEnvelope {
            schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
            debate_id: plan.debate_id.clone(),
            agent: agent_id.to_owned(),
            critique: format!(
                "worker invocation error for {}: {err}",
                adapter.platform().name(),
            ),
            agreement_score: 0,
            dissent_points: vec!["worker_invocation_failure".to_owned()],
            cost_tokens: None,
        },
    };

    let subject = subject_critique(&plan.debate_id, adapter.platform().name());
    let envelope = match Envelope::new(agent_id.to_owned(), subject, critique.to_bytes()) {
        Ok(env) => env,
        Err(_) => return,
    };
    let send = mesh.publish(&envelope);
    if !send_accepted(&send) {
        eprintln!(
            "zornmesh worker: critique publish rejected ({:?} {} {})",
            send.status(),
            send.code(),
            send.message(),
        );
    }
}

fn generate_debate_id() -> String {
    let now = current_unix_ms();
    let counter = NEXT_DEBATE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("deb-{now:x}-{counter:04x}")
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static NEXT_DEBATE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
