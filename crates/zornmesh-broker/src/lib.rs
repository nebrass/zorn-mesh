#![doc = "In-memory broker boundary for first local zornmesh routing work."]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, SystemTime},
};

use zornmesh_core::{
    CoordinationOutcome, CoordinationOutcomeKind, CoordinationStage, DeliveryOutcome, Envelope,
    NackReasonCategory, SubjectValidationError, validate_subject, validate_subject_pattern,
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

    pub fn register_request(
        &self,
        registration: RequestRegistration,
        now: SystemTime,
    ) -> Result<RequestHandle, BrokerError> {
        registration.validate()?;
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.pending_requests.contains_key(&registration.correlation_id) {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                format!(
                    "correlation ID '{}' already has an in-flight request",
                    registration.correlation_id
                ),
            ));
        }
        let deadline = now + registration.timeout;
        let (tx, rx) = mpsc::channel();
        inner.pending_requests.insert(
            registration.correlation_id.clone(),
            PendingRequest {
                registration: registration.clone(),
                deadline,
                resolution_tx: tx,
            },
        );
        Ok(RequestHandle {
            correlation_id: registration.correlation_id,
            receiver: rx,
        })
    }

    pub fn submit_reply(
        &self,
        correlation_id: &str,
        envelope: Envelope,
        now: SystemTime,
    ) -> Result<ReplySubmissionOutcome, BrokerError> {
        if envelope.correlation_id() != correlation_id {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                format!(
                    "reply envelope correlation ID '{}' does not match request '{}'",
                    envelope.correlation_id(),
                    correlation_id
                ),
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(pending) = inner.pending_requests.remove(correlation_id) else {
            if inner.timed_out_correlations.contains(correlation_id) {
                inner.late_events.push(LateRequestEvent::late_after_timeout(
                    correlation_id,
                    now,
                ));
                return Ok(ReplySubmissionOutcome::LateAfterTimeout);
            }
            if inner.completed_correlations.contains(correlation_id) {
                inner
                    .late_events
                    .push(LateRequestEvent::duplicate_after_terminal(
                        correlation_id,
                        now,
                    ));
                return Ok(ReplySubmissionOutcome::DuplicateAfterTerminal);
            }
            inner
                .late_events
                .push(LateRequestEvent::unknown_correlation(correlation_id, now));
            return Ok(ReplySubmissionOutcome::UnknownCorrelation);
        };
        inner
            .completed_correlations
            .insert(correlation_id.to_owned());
        let resolution = RequestResolution::Replied {
            envelope,
            attempt: 1,
        };
        let _ = pending.resolution_tx.send(resolution);
        Ok(ReplySubmissionOutcome::Accepted)
    }

    pub fn submit_request_failure(
        &self,
        correlation_id: &str,
        reason: NackReasonCategory,
        message: impl Into<String>,
        retryable: bool,
        now: SystemTime,
    ) -> Result<ReplySubmissionOutcome, BrokerError> {
        let message = message.into();
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(pending) = inner.pending_requests.remove(correlation_id) else {
            if inner.timed_out_correlations.contains(correlation_id) {
                inner.late_events.push(LateRequestEvent::late_after_timeout(
                    correlation_id,
                    now,
                ));
                return Ok(ReplySubmissionOutcome::LateAfterTimeout);
            }
            if inner.completed_correlations.contains(correlation_id) {
                inner
                    .late_events
                    .push(LateRequestEvent::duplicate_after_terminal(
                        correlation_id,
                        now,
                    ));
                return Ok(ReplySubmissionOutcome::DuplicateAfterTerminal);
            }
            inner
                .late_events
                .push(LateRequestEvent::unknown_correlation(correlation_id, now));
            return Ok(ReplySubmissionOutcome::UnknownCorrelation);
        };
        inner
            .completed_correlations
            .insert(correlation_id.to_owned());
        let outcome = CoordinationOutcome::new(
            CoordinationOutcomeKind::Rejected,
            CoordinationStage::Delivery,
            "REQUEST_REJECTED",
            message,
            retryable,
            true,
            1,
        );
        let _ = pending
            .resolution_tx
            .send(RequestResolution::Rejected { outcome, reason });
        Ok(ReplySubmissionOutcome::Accepted)
    }

    pub fn tick_request_timeouts(&self, now: SystemTime) -> Vec<ExpiredRequest> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let expired_ids: Vec<String> = inner
            .pending_requests
            .iter()
            .filter_map(|(id, pending)| (pending.deadline <= now).then(|| id.clone()))
            .collect();
        let mut expired = Vec::with_capacity(expired_ids.len());
        for id in expired_ids {
            if let Some(pending) = inner.pending_requests.remove(&id) {
                inner.timed_out_correlations.insert(id.clone());
                let outcome = CoordinationOutcome::new(
                    CoordinationOutcomeKind::TimedOut,
                    CoordinationStage::Transport,
                    "REQUEST_TIMED_OUT",
                    format!(
                        "request {id} did not receive a reply within configured timeout"
                    ),
                    true,
                    true,
                    0,
                );
                let _ = pending.resolution_tx.send(RequestResolution::TimedOut {
                    correlation_id: id.clone(),
                    outcome: outcome.clone(),
                });
                expired.push(ExpiredRequest {
                    correlation_id: id,
                    outcome,
                });
            }
        }
        expired
    }

    pub fn late_request_events(&self) -> Vec<LateRequestEvent> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .late_events
            .clone()
    }

    pub fn pending_request_count(&self) -> usize {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .pending_requests
            .len()
    }

    pub fn pending_request_registrations(&self) -> Vec<RequestRegistration> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .pending_requests
            .values()
            .map(|pending| pending.registration.clone())
            .collect()
    }

    pub fn enqueue(
        &self,
        queue: impl Into<String>,
        envelope: Envelope,
    ) -> Result<(), BrokerError> {
        let queue = queue.into();
        if queue.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "queue name must not be empty",
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        inner
            .queues
            .entry(queue)
            .or_default()
            .push_back(QueuedEnvelope {
                envelope,
                attempt: 1,
            });
        Ok(())
    }

    pub fn fetch_leases(
        &self,
        request: FetchRequest,
        now: SystemTime,
    ) -> Result<Vec<Lease>, LeaseError> {
        request.validate()?;
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let mut leases = Vec::new();
        for _ in 0..request.batch_size {
            let item = {
                let queue = inner.queues.entry(request.queue.clone()).or_default();
                queue.pop_front()
            };
            let Some(item) = item else { break };
            inner.next_lease_id += 1;
            let lease_id = format!("lease-{}", inner.next_lease_id);
            let expiry = now + request.lease_duration;
            let lease = Lease {
                lease_id: lease_id.clone(),
                consumer_id: request.consumer_id.clone(),
                queue: request.queue.clone(),
                envelope: item.envelope,
                attempt: item.attempt,
                expiry,
            };
            inner.active_leases.insert(
                lease_id,
                ActiveLease {
                    lease: lease.clone(),
                },
            );
            leases.push(lease);
        }
        Ok(leases)
    }

    pub fn ack_lease(
        &self,
        lease_id: &str,
        consumer_id: &str,
        _now: SystemTime,
    ) -> Result<LeaseAckOutcome, LeaseError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.expired_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseExpired,
                format!("lease {lease_id} has already expired"),
            ));
        }
        if inner.terminal_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseAlreadyTerminal,
                format!("lease {lease_id} is already terminal"),
            ));
        }
        let Some(active) = inner.active_leases.get(lease_id) else {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseUnknown,
                format!("lease {lease_id} is unknown"),
            ));
        };
        if active.lease.consumer_id != consumer_id {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseNotOwned,
                format!(
                    "lease {lease_id} is owned by a different consumer than {consumer_id}"
                ),
            ));
        }
        let lease_id_owned = lease_id.to_owned();
        inner.active_leases.remove(&lease_id_owned);
        inner.terminal_lease_ids.insert(lease_id_owned.clone());
        let outcome = CoordinationOutcome::acknowledged(
            format!("lease {lease_id_owned} acknowledged"),
            1,
        );
        inner.lease_audit.push(LeaseAuditEvent {
            lease_id: lease_id_owned,
            kind: LeaseAuditKind::Acknowledged,
        });
        Ok(LeaseAckOutcome::Acknowledged(outcome))
    }

    pub fn nack_lease(
        &self,
        lease_id: &str,
        consumer_id: &str,
        reason: NackReasonCategory,
        _now: SystemTime,
    ) -> Result<LeaseAckOutcome, LeaseError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.expired_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseExpired,
                format!("lease {lease_id} has already expired"),
            ));
        }
        if inner.terminal_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseAlreadyTerminal,
                format!("lease {lease_id} is already terminal"),
            ));
        }
        let Some(active) = inner.active_leases.get(lease_id) else {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseUnknown,
                format!("lease {lease_id} is unknown"),
            ));
        };
        if active.lease.consumer_id != consumer_id {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseNotOwned,
                format!(
                    "lease {lease_id} is owned by a different consumer than {consumer_id}"
                ),
            ));
        }
        let lease_id_owned = lease_id.to_owned();
        let active = inner.active_leases.remove(&lease_id_owned).expect("lease present");
        inner.terminal_lease_ids.insert(lease_id_owned.clone());
        let queue = inner
            .queues
            .entry(active.lease.queue.clone())
            .or_default();
        queue.push_back(QueuedEnvelope {
            envelope: active.lease.envelope.clone(),
            attempt: active.lease.attempt + 1,
        });
        let outcome = CoordinationOutcome::new(
            CoordinationOutcomeKind::Rejected,
            CoordinationStage::Delivery,
            "LEASE_NACKED",
            format!("lease {lease_id_owned} rejected with reason {}", reason.as_str()),
            true,
            true,
            active.lease.attempt,
        );
        inner.lease_audit.push(LeaseAuditEvent {
            lease_id: lease_id_owned,
            kind: LeaseAuditKind::Nacked(reason),
        });
        Ok(LeaseAckOutcome::Nacked { outcome, reason })
    }

    pub fn renew_lease(
        &self,
        lease_id: &str,
        consumer_id: &str,
        extension: Duration,
        now: SystemTime,
    ) -> Result<LeaseRenewOutcome, LeaseError> {
        if extension.is_zero() {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                "renewal extension must be greater than zero",
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.expired_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseExpired,
                format!("lease {lease_id} has already expired"),
            ));
        }
        if inner.terminal_lease_ids.contains(lease_id) {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseAlreadyTerminal,
                format!("lease {lease_id} is already terminal"),
            ));
        }
        let Some(active) = inner.active_leases.get_mut(lease_id) else {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseUnknown,
                format!("lease {lease_id} is unknown"),
            ));
        };
        if active.lease.consumer_id != consumer_id {
            return Err(LeaseError::new(
                LeaseErrorCode::LeaseNotOwned,
                format!(
                    "lease {lease_id} is owned by a different consumer than {consumer_id}"
                ),
            ));
        }
        let new_expiry = now + extension;
        active.lease.expiry = new_expiry;
        inner.lease_audit.push(LeaseAuditEvent {
            lease_id: lease_id.to_owned(),
            kind: LeaseAuditKind::Renewed,
        });
        Ok(LeaseRenewOutcome::Renewed { new_expiry })
    }

    pub fn expire_due_leases(&self, now: SystemTime) -> Vec<Lease> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let expired_ids: Vec<String> = inner
            .active_leases
            .iter()
            .filter_map(|(id, active)| (active.lease.expiry <= now).then(|| id.clone()))
            .collect();
        let mut expired = Vec::with_capacity(expired_ids.len());
        for id in expired_ids {
            if let Some(active) = inner.active_leases.remove(&id) {
                inner.expired_lease_ids.insert(id.clone());
                let queue = inner
                    .queues
                    .entry(active.lease.queue.clone())
                    .or_default();
                queue.push_back(QueuedEnvelope {
                    envelope: active.lease.envelope.clone(),
                    attempt: active.lease.attempt + 1,
                });
                inner.lease_audit.push(LeaseAuditEvent {
                    lease_id: id.clone(),
                    kind: LeaseAuditKind::Expired,
                });
                expired.push(active.lease);
            }
        }
        expired
    }

    pub fn queue_depth(&self, queue: &str) -> usize {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .queues
            .get(queue)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    pub fn active_lease_count(&self) -> usize {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .active_leases
            .len()
    }

    pub fn lease_audit_events(&self) -> Vec<LeaseAuditEvent> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .lease_audit
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
    pending_requests: HashMap<String, PendingRequest>,
    completed_correlations: HashSet<String>,
    timed_out_correlations: HashSet<String>,
    late_events: Vec<LateRequestEvent>,
    queues: HashMap<String, VecDeque<QueuedEnvelope>>,
    next_lease_id: u64,
    active_leases: HashMap<String, ActiveLease>,
    terminal_lease_ids: HashSet<String>,
    expired_lease_ids: HashSet<String>,
    lease_audit: Vec<LeaseAuditEvent>,
}

#[derive(Debug, Clone)]
struct QueuedEnvelope {
    envelope: Envelope,
    attempt: u32,
}

#[derive(Debug, Clone)]
struct ActiveLease {
    lease: Lease,
}

pub const MAX_FETCH_BATCH: u32 = 1024;
pub const MAX_LEASE_DURATION: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    consumer_id: String,
    queue: String,
    batch_size: u32,
    lease_duration: Duration,
}

impl FetchRequest {
    pub fn new(
        consumer_id: impl Into<String>,
        queue: impl Into<String>,
        batch_size: u32,
        lease_duration: Duration,
    ) -> Self {
        Self {
            consumer_id: consumer_id.into(),
            queue: queue.into(),
            batch_size,
            lease_duration,
        }
    }

    pub fn consumer_id(&self) -> &str {
        &self.consumer_id
    }

    pub fn queue(&self) -> &str {
        &self.queue
    }

    pub const fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub const fn lease_duration(&self) -> Duration {
        self.lease_duration
    }

    fn validate(&self) -> Result<(), LeaseError> {
        if self.consumer_id.trim().is_empty() {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                "consumer ID must not be empty",
            ));
        }
        if self.queue.trim().is_empty() {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                "queue name must not be empty",
            ));
        }
        if self.batch_size == 0 {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                "fetch batch size must be greater than zero",
            ));
        }
        if self.batch_size > MAX_FETCH_BATCH {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                format!(
                    "fetch batch size {} exceeds maximum {MAX_FETCH_BATCH}",
                    self.batch_size
                ),
            ));
        }
        if self.lease_duration.is_zero() {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                "lease duration must be greater than zero",
            ));
        }
        if self.lease_duration > MAX_LEASE_DURATION {
            return Err(LeaseError::new(
                LeaseErrorCode::FetchValidation,
                format!(
                    "lease duration {:?} exceeds maximum {:?}",
                    self.lease_duration, MAX_LEASE_DURATION
                ),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    lease_id: String,
    consumer_id: String,
    queue: String,
    envelope: Envelope,
    attempt: u32,
    expiry: SystemTime,
}

impl Lease {
    pub fn lease_id(&self) -> &str {
        &self.lease_id
    }

    pub fn consumer_id(&self) -> &str {
        &self.consumer_id
    }

    pub fn queue(&self) -> &str {
        &self.queue
    }

    pub const fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    pub const fn expiry(&self) -> SystemTime {
        self.expiry
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseAckOutcome {
    Acknowledged(CoordinationOutcome),
    Nacked {
        outcome: CoordinationOutcome,
        reason: NackReasonCategory,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseRenewOutcome {
    Renewed { new_expiry: SystemTime },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseErrorCode {
    FetchValidation,
    LeaseUnknown,
    LeaseNotOwned,
    LeaseExpired,
    LeaseAlreadyTerminal,
}

impl LeaseErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FetchValidation => "E_FETCH_VALIDATION",
            Self::LeaseUnknown => "E_LEASE_UNKNOWN",
            Self::LeaseNotOwned => "E_LEASE_NOT_OWNED",
            Self::LeaseExpired => "E_LEASE_EXPIRED",
            Self::LeaseAlreadyTerminal => "E_LEASE_ALREADY_TERMINAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseError {
    code: LeaseErrorCode,
    message: String,
}

impl LeaseError {
    fn new(code: LeaseErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> LeaseErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for LeaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for LeaseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseAuditKind {
    Acknowledged,
    Nacked(NackReasonCategory),
    Renewed,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseAuditEvent {
    lease_id: String,
    kind: LeaseAuditKind,
}

impl LeaseAuditEvent {
    pub fn lease_id(&self) -> &str {
        &self.lease_id
    }

    pub const fn kind(&self) -> LeaseAuditKind {
        self.kind
    }
}

#[derive(Debug)]
struct PendingRequest {
    registration: RequestRegistration,
    deadline: SystemTime,
    resolution_tx: Sender<RequestResolution>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRegistration {
    correlation_id: String,
    source_agent: String,
    target_agent: String,
    subject: String,
    timeout: Duration,
}

impl RequestRegistration {
    pub fn new(
        correlation_id: impl Into<String>,
        source_agent: impl Into<String>,
        target_agent: impl Into<String>,
        subject: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            correlation_id: correlation_id.into(),
            source_agent: source_agent.into(),
            target_agent: target_agent.into(),
            subject: subject.into(),
            timeout,
        }
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn source_agent(&self) -> &str {
        &self.source_agent
    }

    pub fn target_agent(&self) -> &str {
        &self.target_agent
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    fn validate(&self) -> Result<(), BrokerError> {
        if self.correlation_id.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "request correlation ID must not be empty",
            ));
        }
        if self.source_agent.trim().is_empty() || self.target_agent.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "request source and target agents must not be empty",
            ));
        }
        if self.timeout.is_zero() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "request timeout must be greater than zero",
            ));
        }
        validate_subject(&self.subject).map_err(BrokerError::from)
    }
}

#[derive(Debug)]
pub struct RequestHandle {
    correlation_id: String,
    receiver: Receiver<RequestResolution>,
}

impl RequestHandle {
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<RequestResolution, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestResolution {
    Replied {
        envelope: Envelope,
        attempt: u32,
    },
    Rejected {
        outcome: CoordinationOutcome,
        reason: NackReasonCategory,
    },
    TimedOut {
        correlation_id: String,
        outcome: CoordinationOutcome,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplySubmissionOutcome {
    Accepted,
    DuplicateAfterTerminal,
    LateAfterTimeout,
    UnknownCorrelation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiredRequest {
    pub correlation_id: String,
    pub outcome: CoordinationOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LateRequestKind {
    DuplicateAfterTerminal,
    LateAfterTimeout,
    UnknownCorrelation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LateRequestEvent {
    correlation_id: String,
    kind: LateRequestKind,
    observed_at: SystemTime,
}

impl LateRequestEvent {
    fn duplicate_after_terminal(correlation_id: &str, observed_at: SystemTime) -> Self {
        Self {
            correlation_id: correlation_id.to_owned(),
            kind: LateRequestKind::DuplicateAfterTerminal,
            observed_at,
        }
    }

    fn late_after_timeout(correlation_id: &str, observed_at: SystemTime) -> Self {
        Self {
            correlation_id: correlation_id.to_owned(),
            kind: LateRequestKind::LateAfterTimeout,
            observed_at,
        }
    }

    fn unknown_correlation(correlation_id: &str, observed_at: SystemTime) -> Self {
        Self {
            correlation_id: correlation_id.to_owned(),
            kind: LateRequestKind::UnknownCorrelation,
            observed_at,
        }
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub const fn kind(&self) -> LateRequestKind {
        self.kind
    }

    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }
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
