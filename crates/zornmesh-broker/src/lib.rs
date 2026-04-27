#![doc = "In-memory broker boundary for first local zornmesh routing work."]

use std::{
    fmt,
    sync::{Arc, Mutex, mpsc::Sender},
};

use zornmesh_core::{
    CoordinationOutcome, DeliveryOutcome, Envelope, NackReasonCategory, SubjectValidationError,
    validate_subject, validate_subject_pattern,
};

pub const CRATE_BOUNDARY: &str = "zornmesh-broker";
pub const MAX_SUBSCRIBERS_PER_PATTERN: usize = 256;
pub const MAX_TOTAL_SUBSCRIPTIONS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerBoundary;

impl BrokerBoundary {
    pub const fn name(self) -> &'static str {
        CRATE_BOUNDARY
    }
}

#[derive(Debug, Clone)]
pub struct Broker {
    inner: Arc<Mutex<BrokerInner>>,
}

impl Broker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrokerInner::default())),
        }
    }

    pub fn subscribe(
        &self,
        pattern: impl Into<String>,
        delivery_tx: Sender<DeliveryAttempt>,
    ) -> Result<Subscription, BrokerError> {
        let pattern = SubjectPattern::new(pattern)?;
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.subscriptions.len() >= MAX_TOTAL_SUBSCRIPTIONS {
            return Err(BrokerError::new(
                BrokerErrorCode::SubscriptionCap,
                format!("subscription cap exceeded: maximum {MAX_TOTAL_SUBSCRIPTIONS} total"),
            ));
        }

        let same_pattern_count = inner
            .subscriptions
            .iter()
            .filter(|subscription| subscription.pattern == pattern)
            .count();
        if same_pattern_count >= MAX_SUBSCRIBERS_PER_PATTERN {
            return Err(BrokerError::new(
                BrokerErrorCode::SubscriptionCap,
                format!(
                    "subscription cap exceeded for pattern '{}': maximum {MAX_SUBSCRIBERS_PER_PATTERN}",
                    pattern.as_str()
                ),
            ));
        }

        let id = inner.next_subscription_id;
        inner.next_subscription_id += 1;
        inner.subscriptions.push(SubscriptionEntry {
            id,
            pattern,
            delivery_tx,
        });

        Ok(Subscription {
            id,
            inner: Arc::clone(&self.inner),
        })
    }

    pub fn publish(&self, envelope: Envelope) -> Result<PublishReceipt, BrokerError> {
        validate_subject(envelope.subject()).map_err(BrokerError::from)?;
        let inner = self.inner.lock().expect("broker lock is not poisoned");
        let delivery = DeliveryAttempt::new(envelope, 1);
        let mut delivered = 0;

        for subscription in inner
            .subscriptions
            .iter()
            .filter(|subscription| subscription.pattern.matches(delivery.envelope().subject()))
        {
            if subscription.delivery_tx.send(delivery.clone()).is_ok() {
                delivered += 1;
            }
        }

        let delivery_attempts =
            u32::try_from(delivered).expect("subscriber caps fit in coordination delivery attempts");
        Ok(PublishReceipt::new(delivery_attempts))
    }

    pub fn subscription_count(&self) -> usize {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .subscriptions
            .len()
    }

    pub fn record_ack(&self, delivery_id: impl Into<String>) -> Result<DeliveryOutcome, BrokerError> {
        let delivery_id = delivery_id.into();
        validate_delivery_id(&delivery_id)?;
        let outcome = DeliveryOutcome::acknowledged(delivery_id);
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .delivery_outcomes
            .push(outcome.clone());
        Ok(outcome)
    }

    pub fn record_nack(
        &self,
        delivery_id: impl Into<String>,
        reason: NackReasonCategory,
    ) -> Result<DeliveryOutcome, BrokerError> {
        let delivery_id = delivery_id.into();
        validate_delivery_id(&delivery_id)?;
        let outcome = DeliveryOutcome::rejected(delivery_id, reason);
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .delivery_outcomes
            .push(outcome.clone());
        Ok(outcome)
    }

    pub fn delivery_outcomes(&self) -> Vec<DeliveryOutcome> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .delivery_outcomes
            .clone()
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct BrokerInner {
    next_subscription_id: u64,
    subscriptions: Vec<SubscriptionEntry>,
    delivery_outcomes: Vec<DeliveryOutcome>,
}

#[derive(Debug)]
struct SubscriptionEntry {
    id: u64,
    pattern: SubjectPattern,
    delivery_tx: Sender<DeliveryAttempt>,
}

#[derive(Debug)]
pub struct Subscription {
    id: u64,
    inner: Arc<Mutex<BrokerInner>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        inner
            .subscriptions
            .retain(|subscription| subscription.id != self.id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAttempt {
    delivery_id: String,
    envelope: Envelope,
    attempt: u32,
}

impl DeliveryAttempt {
    pub fn new(envelope: Envelope, attempt: u32) -> Self {
        let delivery_id = format!("{}:{attempt}", envelope.correlation_id());
        Self {
            delivery_id,
            envelope,
            attempt,
        }
    }

    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }

    pub const fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    transport_outcome: CoordinationOutcome,
    durable_outcome: CoordinationOutcome,
    delivery_attempts: u32,
}

impl PublishReceipt {
    fn new(delivery_attempts: u32) -> Self {
        Self {
            transport_outcome: CoordinationOutcome::accepted(
                format!("accepted for routing; delivery_attempts={delivery_attempts}"),
                delivery_attempts,
            ),
            durable_outcome: CoordinationOutcome::persistence_unavailable(),
            delivery_attempts,
        }
    }

    pub const fn transport_outcome(&self) -> &CoordinationOutcome {
        &self.transport_outcome
    }

    pub const fn durable_outcome(&self) -> &CoordinationOutcome {
        &self.durable_outcome
    }

    pub const fn delivery_attempts(&self) -> u32 {
        self.delivery_attempts
    }
}

impl PartialEq<usize> for PublishReceipt {
    fn eq(&self, other: &usize) -> bool {
        usize::try_from(self.delivery_attempts) == Ok(*other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectPattern {
    raw: String,
    levels: Vec<String>,
}

impl SubjectPattern {
    pub fn new(value: impl Into<String>) -> Result<Self, BrokerError> {
        let raw = value.into();
        validate_subject_pattern(&raw).map_err(BrokerError::from)?;
        let levels = raw.split('.').map(ToOwned::to_owned).collect();
        Ok(Self { raw, levels })
    }

    pub fn matches(&self, subject: &str) -> bool {
        if validate_subject(subject).is_err() {
            return false;
        }

        let subject_levels = subject.split('.').collect::<Vec<_>>();
        let mut subject_index = 0;
        for pattern_level in &self.levels {
            if pattern_level == ">" {
                return true;
            }
            if subject_index >= subject_levels.len() {
                return false;
            }
            if pattern_level != "*" && pattern_level != subject_levels[subject_index] {
                return false;
            }
            subject_index += 1;
        }

        subject_index == subject_levels.len()
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerErrorCode {
    SubjectValidation,
    SubscriptionCap,
    DeliveryValidation,
}

impl BrokerErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubjectValidation => "E_SUBJECT_VALIDATION",
            Self::SubscriptionCap => "E_SUBSCRIPTION_CAP",
            Self::DeliveryValidation => "E_DELIVERY_VALIDATION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerError {
    code: BrokerErrorCode,
    message: String,
}

impl BrokerError {
    fn new(code: BrokerErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> BrokerErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<SubjectValidationError> for BrokerError {
    fn from(value: SubjectValidationError) -> Self {
        Self::new(BrokerErrorCode::SubjectValidation, value.to_string())
    }
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for BrokerError {}

fn validate_delivery_id(delivery_id: &str) -> Result<(), BrokerError> {
    if delivery_id.trim().is_empty() {
        return Err(BrokerError::new(
            BrokerErrorCode::DeliveryValidation,
            "delivery ID must not be empty",
        ));
    }
    Ok(())
}
