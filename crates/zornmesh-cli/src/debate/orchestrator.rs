//! Synchronous orchestrator used by the MCP `tools/call` handler and the
//! `zornmesh debate run` CLI command.
//!
//! Single-round v0.2 orchestrator: publish plan, drain critiques until quorum
//! or timeout, synthesize consensus + dissent, return.

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, channel},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::broker::{Broker, BrokerError, PeerCredentials, SocketTrustPolicy, Subscription};
use crate::core::Envelope;

use super::{
    ConsensusEnvelope, CritiqueEnvelope, DEBATE_SCHEMA_VERSION, DissentRecord, PlanEnvelope,
    subject_consensus, subject_critique_pattern, subject_plan,
};

/// User-tunable knobs for a single debate run.
#[derive(Debug, Clone)]
pub struct DebateOptions {
    pub originator: String,
    pub plan: String,
    pub repo: Option<String>,
    pub timeout: Duration,
    /// Minimum critiques to consider the debate complete before timeout.
    /// Set to 1 for "first response wins"; set higher for quorum.
    pub quorum: u32,
    pub max_tokens: Option<u64>,
}

impl DebateOptions {
    pub fn new(originator: impl Into<String>, plan: impl Into<String>) -> Self {
        Self {
            originator: originator.into(),
            plan: plan.into(),
            repo: None,
            timeout: Duration::from_secs(30),
            quorum: 1,
            max_tokens: None,
        }
    }

    pub fn with_repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_quorum(mut self, quorum: u32) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u64) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// Outcome returned to the caller.
#[derive(Debug, Clone)]
pub struct DebateOutcome {
    pub debate_id: String,
    pub plan: String,
    pub critiques: Vec<CritiqueEnvelope>,
    pub consensus: ConsensusEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebateError {
    InvalidPlan(String),
    BrokerFailure(String),
    PublishFailure(String),
}

impl DebateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPlan(_) => "E_DEBATE_INVALID_PLAN",
            Self::BrokerFailure(_) => "E_DEBATE_BROKER_FAILURE",
            Self::PublishFailure(_) => "E_DEBATE_PUBLISH_FAILURE",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidPlan(m) | Self::BrokerFailure(m) | Self::PublishFailure(m) => m,
        }
    }
}

impl From<BrokerError> for DebateError {
    fn from(error: BrokerError) -> Self {
        DebateError::BrokerFailure(format!("{}: {}", error.code().as_str(), error.message()))
    }
}

/// Runs a single debate end-to-end. Created and consumed within one
/// `tools/call` (MCP) or one CLI invocation; not reused across debates.
pub struct DebateOrchestrator<'a> {
    broker: &'a Broker,
    /// Credentials the orchestrator publishes under. v0.2 uses the bridge's
    /// own credentials; future versions might split into a dedicated
    /// `agent.orchestrator.<id>` identity for trust scoping.
    #[allow(dead_code)]
    credentials: PeerCredentials,
    #[allow(dead_code)]
    trust_policy: SocketTrustPolicy,
}

impl<'a> DebateOrchestrator<'a> {
    pub fn new(
        broker: &'a Broker,
        credentials: PeerCredentials,
        trust_policy: SocketTrustPolicy,
    ) -> Self {
        Self {
            broker,
            credentials,
            trust_policy,
        }
    }

    pub fn run(&self, options: DebateOptions) -> Result<DebateOutcome, DebateError> {
        if options.plan.trim().is_empty() {
            return Err(DebateError::InvalidPlan(
                "plan must be a non-empty string".to_owned(),
            ));
        }

        let debate_id = generate_debate_id();
        let plan_subject = subject_plan(&debate_id);
        let critique_pattern = subject_critique_pattern(&debate_id);
        let consensus_subject = subject_consensus(&debate_id);
        let deadline_unix_ms = current_unix_ms() + options.timeout.as_millis() as u64;

        // 1. Subscribe BEFORE publishing the plan so we don't race a fast worker.
        let (tx, rx) = channel();
        let _subscription: Subscription = self.broker.subscribe(critique_pattern.clone(), tx)?;

        // 2. Publish the plan.
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
        .map_err(|err| DebateError::PublishFailure(format!("envelope: {err:?}")))?;
        self.broker.publish(plan_envelope)?;

        // 3. Drain critiques until quorum or timeout.
        let critiques = collect_critiques(&rx, options.timeout, options.quorum, &debate_id);

        // 4. Synthesize consensus + publish.
        let consensus = synthesize(&debate_id, &options.plan, &critiques, 1);
        let consensus_envelope = Envelope::new(
            options.originator.clone(),
            consensus_subject,
            consensus.to_bytes(),
        )
        .map_err(|err| DebateError::PublishFailure(format!("consensus envelope: {err:?}")))?;
        // A publish failure on the consensus subject doesn't ruin the run --
        // we still return the synthesized outcome to the caller. Audit /
        // observability tooling will surface the publish failure separately.
        let _ = self.broker.publish(consensus_envelope);

        Ok(DebateOutcome {
            debate_id,
            plan: options.plan,
            critiques,
            consensus,
        })
    }
}

fn collect_critiques(
    rx: &Receiver<crate::broker::DeliveryAttempt>,
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
        match rx.recv_timeout(remaining) {
            Ok(delivery) => {
                if let Ok(critique) = CritiqueEnvelope::from_bytes(delivery.envelope().payload()) {
                    if critique.debate_id == debate_id {
                        // Dedupe by agent; first response per agent wins.
                        if !critiques.iter().any(|c| c.agent == critique.agent) {
                            critiques.push(critique);
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
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

    // Synthesis strategy for v0.2: include the original plan, a per-agent
    // critique digest, and an explicit dissent block. We deliberately do NOT
    // average or paper over disagreements -- the originator sees them.
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
