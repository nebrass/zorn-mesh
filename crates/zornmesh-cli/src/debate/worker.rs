//! Long-lived worker daemon: subscribes to debate plans, drives the
//! underlying coding agent in non-interactive mode, publishes a critique
//! envelope back. One worker per platform per machine.

use std::{
    sync::mpsc::channel,
    thread,
    time::{Duration, Instant},
};

use crate::broker::{Broker, DeliveryAttempt, Subscription};
use crate::core::Envelope;

use super::{
    CritiqueEnvelope, DEBATE_SCHEMA_VERSION, PlanEnvelope, PlatformAdapter,
    WORKER_PLAN_SUBSCRIPTION, platforms, subject_critique,
};

/// Owns the platform adapter, the broker subscription, and the per-invocation
/// budget. Spawns no internal threads in v0.2 -- one delivery is processed
/// at a time (good enough for interactive use; v0.3 can add concurrency).
pub struct WorkerDaemon<'a> {
    broker: &'a Broker,
    adapter: Box<dyn PlatformAdapter>,
    agent_id: String,
    invocation_timeout: Duration,
}

impl<'a> WorkerDaemon<'a> {
    pub fn new(broker: &'a Broker, platform: platforms::Platform) -> Self {
        let adapter = platforms::adapter_for(platform);
        let agent_id = format!("agent.worker.{}", platform.name());
        Self {
            broker,
            adapter,
            agent_id,
            invocation_timeout: Duration::from_secs(120),
        }
    }

    pub fn with_invocation_timeout(mut self, timeout: Duration) -> Self {
        self.invocation_timeout = timeout;
        self
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Runs the listen loop. Returns when the broker subscription drops or
    /// when `stop_after` (a test/CI knob) is reached. In production callers
    /// pass `None` and rely on SIGTERM (the broker is the daemon process).
    pub fn listen(&self, stop_after: Option<Duration>) -> Result<(), String> {
        let (tx, rx) = channel::<DeliveryAttempt>();
        let _subscription: Subscription = self
            .broker
            .subscribe(WORKER_PLAN_SUBSCRIPTION, tx)
            .map_err(|err| format!("subscribe failed: {}", err.code().as_str()))?;

        let started = Instant::now();
        loop {
            if let Some(stop) = stop_after {
                if started.elapsed() >= stop {
                    return Ok(());
                }
            }
            let recv_timeout = stop_after
                .map(|stop| stop.saturating_sub(started.elapsed()))
                .unwrap_or(Duration::from_secs(60));
            match rx.recv_timeout(recv_timeout) {
                Ok(delivery) => {
                    self.handle_plan_delivery(delivery);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if stop_after.is_some() {
                        return Ok(());
                    }
                    // Production loop: just keep waiting. The broker keeps
                    // the subscription alive; nothing to do on timeout.
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("broker subscription disconnected".to_owned());
                }
            }
        }
    }

    fn handle_plan_delivery(&self, delivery: DeliveryAttempt) {
        let plan = match PlanEnvelope::from_bytes(delivery.envelope().payload()) {
            Ok(plan) => plan,
            Err(_) => return, // Malformed plan -- skip silently. (v0.3: emit a structured error envelope.)
        };

        // Don't critique your own plans.
        if plan.originator == self.agent_id {
            return;
        }

        let prompt = build_critique_prompt(&plan);
        let invocation_result =
            self.adapter
                .invoke(&prompt, plan.repo.as_deref(), self.invocation_timeout);

        let critique = match invocation_result {
            Ok(invocation) => {
                let body = if invocation.stdout.trim().is_empty() {
                    format!(
                        "(empty response from {} CLI; exit_status={:?})",
                        self.adapter.platform().name(),
                        invocation.exit_status,
                    )
                } else {
                    invocation.stdout.trim().to_owned()
                };
                CritiqueEnvelope {
                    schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
                    debate_id: plan.debate_id.clone(),
                    agent: self.agent_id.clone(),
                    critique: body,
                    agreement_score: 50, // Calibration is a v0.3+ concern.
                    dissent_points: Vec::new(),
                    cost_tokens: None,
                }
            }
            Err(err) => CritiqueEnvelope {
                schema_version: DEBATE_SCHEMA_VERSION.to_owned(),
                debate_id: plan.debate_id.clone(),
                agent: self.agent_id.clone(),
                critique: format!(
                    "worker invocation error for {}: {err}",
                    self.adapter.platform().name(),
                ),
                agreement_score: 0,
                dissent_points: vec!["worker_invocation_failure".to_owned()],
                cost_tokens: None,
            },
        };

        let subject = subject_critique(&plan.debate_id, self.adapter.platform().name());
        let envelope = match Envelope::new(self.agent_id.clone(), subject, critique.to_bytes()) {
            Ok(env) => env,
            Err(_) => return,
        };
        let _ = self.broker.publish(envelope);
    }
}

/// Construct the critique prompt sent to the underlying coding agent. v0.2
/// uses a simple template; v0.3+ can swap to a per-platform tailored
/// version (Claude's prompt-caching, Copilot's repo-scoped grounding, etc.).
pub(crate) fn build_critique_prompt(plan: &PlanEnvelope) -> String {
    let repo_line = plan
        .repo
        .as_deref()
        .map(|r| format!("Repo context: {r}\n"))
        .unwrap_or_default();
    format!(
        "You are participating in a multi-agent debate over a coding plan.\n\
         Originator: {}\n\
         {repo_line}\
         \n\
         Plan:\n{}\n\
         \n\
         Please provide a focused critique in 4-8 sentences:\n\
         - What's correct about this plan.\n\
         - What's missing or risky.\n\
         - One specific improvement.\n\
         Keep your response concrete; do not reformulate the plan.\n",
        plan.originator, plan.plan,
    )
}

/// Spawn the daemon in a background thread; returns the join handle so the
/// caller can decide when to wait. v0.2 is single-platform per process; the
/// CLI invokes this once per `zornmesh worker` subcommand.
pub fn spawn_listener(
    broker: Broker,
    platform: platforms::Platform,
    stop_after: Option<Duration>,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        let daemon = WorkerDaemon::new(&broker, platform);
        daemon.listen(stop_after)
    })
}
