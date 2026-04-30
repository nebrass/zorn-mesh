//! Wire envelopes for the debate subsystem.
//!
//! We hand-roll JSON serialization (rather than depend on `serde_derive`) because the
//! workspace already pins `serde_json` as the only serialization dep, and adding a
//! proc-macro crate would expand build times for a handful of straightforward structs.

use serde_json::{Map, Value, json};

/// Schema version pinned in every debate envelope so older replays remain
/// interpretable when the field set evolves. Mirrors the existing
/// `zornmesh.cli.read.v1` pattern from the audit/inspect subsystem.
pub const DEBATE_SCHEMA_VERSION: &str = "zornmesh.debate.v1";

/// What a driver (a coding agent acting on a user's behalf) broadcasts to start a debate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEnvelope {
    pub schema_version: String,
    pub debate_id: String,
    pub originator: String,
    pub plan: String,
    pub repo: Option<String>,
    pub deadline_unix_ms: u64,
    pub max_tokens: Option<u64>,
}

impl PlanEnvelope {
    pub fn to_json(&self) -> Value {
        let mut payload = Map::new();
        payload.insert(
            "schema_version".to_owned(),
            Value::String(self.schema_version.clone()),
        );
        payload.insert("kind".to_owned(), Value::String("plan".to_owned()));
        payload.insert(
            "debate_id".to_owned(),
            Value::String(self.debate_id.clone()),
        );
        payload.insert(
            "originator".to_owned(),
            Value::String(self.originator.clone()),
        );
        payload.insert("plan".to_owned(), Value::String(self.plan.clone()));
        if let Some(repo) = &self.repo {
            payload.insert("repo".to_owned(), Value::String(repo.clone()));
        }
        payload.insert(
            "deadline_unix_ms".to_owned(),
            Value::Number(self.deadline_unix_ms.into()),
        );
        if let Some(tokens) = self.max_tokens {
            payload.insert("max_tokens".to_owned(), Value::Number(tokens.into()));
        }
        Value::Object(payload)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.to_json()).expect("plan envelope is JSON-serializable")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EnvelopeDecodeError> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|err| EnvelopeDecodeError::new("invalid JSON", err.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| EnvelopeDecodeError::new("plan envelope must be object", String::new()))?;
        let kind_ok = object.get("kind").and_then(Value::as_str) == Some("plan");
        if !kind_ok {
            return Err(EnvelopeDecodeError::new(
                "envelope kind is not 'plan'",
                String::new(),
            ));
        }
        Ok(Self {
            schema_version: required_str(object, "schema_version")?,
            debate_id: required_str(object, "debate_id")?,
            originator: required_str(object, "originator")?,
            plan: required_str(object, "plan")?,
            repo: object
                .get("repo")
                .and_then(Value::as_str)
                .map(str::to_owned),
            deadline_unix_ms: required_u64(object, "deadline_unix_ms")?,
            max_tokens: object.get("max_tokens").and_then(Value::as_u64),
        })
    }
}

/// What a worker publishes after invoking its underlying coding agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CritiqueEnvelope {
    pub schema_version: String,
    pub debate_id: String,
    pub agent: String,
    pub critique: String,
    /// 0..=100. Higher means stronger agreement with the original plan.
    pub agreement_score: u8,
    pub dissent_points: Vec<String>,
    pub cost_tokens: Option<u64>,
}

impl CritiqueEnvelope {
    pub fn to_json(&self) -> Value {
        let dissent: Vec<Value> = self
            .dissent_points
            .iter()
            .map(|p| Value::String(p.clone()))
            .collect();
        let mut payload = Map::new();
        payload.insert(
            "schema_version".to_owned(),
            Value::String(self.schema_version.clone()),
        );
        payload.insert("kind".to_owned(), Value::String("critique".to_owned()));
        payload.insert(
            "debate_id".to_owned(),
            Value::String(self.debate_id.clone()),
        );
        payload.insert("agent".to_owned(), Value::String(self.agent.clone()));
        payload.insert("critique".to_owned(), Value::String(self.critique.clone()));
        payload.insert(
            "agreement_score".to_owned(),
            Value::Number(self.agreement_score.into()),
        );
        payload.insert("dissent_points".to_owned(), Value::Array(dissent));
        if let Some(tokens) = self.cost_tokens {
            payload.insert("cost_tokens".to_owned(), Value::Number(tokens.into()));
        }
        Value::Object(payload)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.to_json()).expect("critique envelope is JSON-serializable")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EnvelopeDecodeError> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|err| EnvelopeDecodeError::new("invalid JSON", err.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| EnvelopeDecodeError::new("critique envelope must be object", String::new()))?;
        let kind_ok = object.get("kind").and_then(Value::as_str) == Some("critique");
        if !kind_ok {
            return Err(EnvelopeDecodeError::new(
                "envelope kind is not 'critique'",
                String::new(),
            ));
        }
        let dissent_points = object
            .get("dissent_points")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let agreement_score = object
            .get("agreement_score")
            .and_then(Value::as_u64)
            .unwrap_or(50);
        let agreement_score = u8::try_from(agreement_score).unwrap_or(100);
        Ok(Self {
            schema_version: required_str(object, "schema_version")?,
            debate_id: required_str(object, "debate_id")?,
            agent: required_str(object, "agent")?,
            critique: required_str(object, "critique")?,
            agreement_score,
            dissent_points,
            cost_tokens: object.get("cost_tokens").and_then(Value::as_u64),
        })
    }
}

/// What the orchestrator returns to the originator (and writes to the consensus subject).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsensusEnvelope {
    pub schema_version: String,
    pub debate_id: String,
    pub consensus: String,
    pub dissent: Vec<DissentRecord>,
    pub participants: Vec<String>,
    pub timed_out: Vec<String>,
    pub total_cost_tokens: u64,
    pub round: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DissentRecord {
    pub agent: String,
    pub points: Vec<String>,
}

impl ConsensusEnvelope {
    pub fn to_json(&self) -> Value {
        let dissent: Vec<Value> = self
            .dissent
            .iter()
            .map(|d| {
                json!({
                    "agent": d.agent,
                    "points": d.points,
                })
            })
            .collect();
        let participants: Vec<Value> = self
            .participants
            .iter()
            .map(|p| Value::String(p.clone()))
            .collect();
        let timed_out: Vec<Value> = self
            .timed_out
            .iter()
            .map(|p| Value::String(p.clone()))
            .collect();
        json!({
            "schema_version": self.schema_version,
            "kind": "consensus",
            "debate_id": self.debate_id,
            "consensus": self.consensus,
            "dissent": dissent,
            "participants": participants,
            "timed_out": timed_out,
            "total_cost_tokens": self.total_cost_tokens,
            "round": self.round,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.to_json()).expect("consensus envelope is JSON-serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeDecodeError {
    pub message: String,
    pub detail: String,
}

impl EnvelopeDecodeError {
    fn new(message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            detail: detail.into(),
        }
    }
}

fn required_str(object: &Map<String, Value>, key: &str) -> Result<String, EnvelopeDecodeError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            EnvelopeDecodeError::new(format!("missing required field '{key}'"), String::new())
        })
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, EnvelopeDecodeError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            EnvelopeDecodeError::new(
                format!("missing or non-u64 field '{key}'"),
                String::new(),
            )
        })
}
