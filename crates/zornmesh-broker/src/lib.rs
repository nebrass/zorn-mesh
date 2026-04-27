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
    AgentCard, CapabilityDescriptor, CapabilityDirection, CoordinationOutcome,
    CoordinationOutcomeKind, CoordinationStage, DeliveryOutcome, Envelope, NackReasonCategory,
    SubjectValidationError, validate_subject, validate_subject_pattern,
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

    pub fn register_agent_card(
        &self,
        card: AgentCard,
    ) -> Result<AgentRegistrationOutcome, BrokerError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let canonical_id = card.canonical_stable_id().to_owned();
        if let Some(existing) = inner.agent_cards.get(&canonical_id) {
            if existing.is_compatible_with(&card) {
                return Ok(AgentRegistrationOutcome::Compatible {
                    canonical: existing.clone(),
                });
            }
            return Ok(AgentRegistrationOutcome::Conflict {
                existing: existing.clone(),
                attempted: card,
            });
        }
        inner.agent_cards.insert(canonical_id, card.clone());
        Ok(AgentRegistrationOutcome::Registered { canonical: card })
    }

    pub fn declare_capabilities(
        &self,
        agent_canonical_id: &str,
        descriptors: Vec<CapabilityDescriptor>,
    ) -> Result<CapabilityDeclarationOutcome, CapabilityError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if !inner.agent_cards.contains_key(agent_canonical_id) {
            return Err(CapabilityError::new(
                CapabilityErrorCode::AgentNotFound,
                format!("agent '{agent_canonical_id}' is not registered"),
            ));
        }
        let mut offered = Vec::new();
        let mut consumed = Vec::new();
        for descriptor in descriptors {
            match descriptor.direction() {
                CapabilityDirection::Offered => offered.push(descriptor),
                CapabilityDirection::Consumed => consumed.push(descriptor),
                CapabilityDirection::Both => {
                    offered.push(descriptor.clone());
                    consumed.push(descriptor);
                }
            }
        }

        let change_kind = if inner.capabilities.contains_key(agent_canonical_id) {
            CapabilityChangeKind::Changed
        } else {
            CapabilityChangeKind::Initial
        };
        inner.capabilities.insert(
            agent_canonical_id.to_owned(),
            AgentCapabilities {
                offered: offered.clone(),
                consumed: consumed.clone(),
            },
        );
        inner.capability_change_events.push(CapabilityChangeEvent {
            agent_canonical_id: agent_canonical_id.to_owned(),
            kind: change_kind,
            offered_count: offered.len(),
            consumed_count: consumed.len(),
        });
        Ok(CapabilityDeclarationOutcome::Updated {
            offered,
            consumed,
            change_kind,
        })
    }

    pub fn inspect_agent_capabilities(
        &self,
        agent_canonical_id: &str,
    ) -> Option<AgentCapabilitySummary> {
        let inner = self.inner.lock().expect("broker lock is not poisoned");
        let agent = inner.agent_cards.get(agent_canonical_id)?.clone();
        let caps = inner.capabilities.get(agent_canonical_id).cloned()?;
        Some(AgentCapabilitySummary {
            agent,
            offered: caps.offered,
            consumed: caps.consumed,
        })
    }

    pub fn list_agents_with_capabilities(&self) -> Vec<AgentCapabilitySummary> {
        let inner = self.inner.lock().expect("broker lock is not poisoned");
        inner
            .agent_cards
            .iter()
            .map(|(id, agent)| {
                let caps = inner.capabilities.get(id).cloned().unwrap_or_default();
                AgentCapabilitySummary {
                    agent: agent.clone(),
                    offered: caps.offered,
                    consumed: caps.consumed,
                }
            })
            .collect()
    }

    pub fn capability_change_events(&self) -> Vec<CapabilityChangeEvent> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .capability_change_events
            .clone()
    }

    pub fn lookup_agent_card(&self, canonical_stable_id: &str) -> Option<AgentCard> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .agent_cards
            .get(canonical_stable_id)
            .cloned()
    }

    pub fn configure_queue_bounds(
        &self,
        queue: impl Into<String>,
        config: QueueBoundsConfig,
    ) -> Result<(), BrokerError> {
        let queue = queue.into();
        if queue.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "queue name must not be empty",
            ));
        }
        if config.max_depth == 0 {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "queue max_depth must be greater than zero",
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        inner.queue_bounds.insert(queue, config);
        Ok(())
    }

    pub fn publish_with_backpressure(
        &self,
        queue: impl Into<String>,
        envelope: Envelope,
    ) -> Result<BackpressureOutcome, BrokerError> {
        let queue = queue.into();
        if queue.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "queue name must not be empty",
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let config = inner.queue_bounds.get(&queue).copied();
        let queued = inner.queues.entry(queue.clone()).or_default();
        let depth = queued.len();
        let Some(config) = config else {
            queued.push_back(QueuedEnvelope {
                envelope,
                attempt: 1,
            });
            return Ok(BackpressureOutcome::Accepted);
        };
        if depth < config.max_depth {
            queued.push_back(QueuedEnvelope {
                envelope,
                attempt: 1,
            });
            return Ok(BackpressureOutcome::Accepted);
        }
        let details = BackpressureDetails {
            subject_scope: queue.clone(),
            queue_bound: config.max_depth,
            exceeded_limit: depth,
            retryable: true,
            suggested_delay: Duration::from_millis(50),
            remediation: format!(
                "queue '{queue}' at bound {} envelopes; reduce publish rate or grow consumer capacity",
                config.max_depth
            ),
        };
        match config.drop_policy {
            QueueDropPolicy::Reject => Ok(BackpressureOutcome::RejectedBackpressure { details }),
            QueueDropPolicy::DropOldest => {
                let dropped = queued.pop_front();
                queued.push_back(QueuedEnvelope {
                    envelope,
                    attempt: 1,
                });
                Ok(BackpressureOutcome::DroppedByPolicy {
                    policy: QueueDropPolicy::DropOldest,
                    details,
                    dropped_correlation_id: dropped
                        .map(|item| item.envelope.correlation_id().to_owned()),
                })
            }
            QueueDropPolicy::DropNewest => Ok(BackpressureOutcome::DroppedByPolicy {
                policy: QueueDropPolicy::DropNewest,
                details,
                dropped_correlation_id: Some(envelope.correlation_id().to_owned()),
            }),
        }
    }

    pub fn record_consumer_health_signal(
        &self,
        consumer_id: &str,
        signal: ConsumerHealthSignal,
    ) -> Result<ConsumerHealthState, BrokerError> {
        if consumer_id.trim().is_empty() {
            return Err(BrokerError::new(
                BrokerErrorCode::DeliveryValidation,
                "consumer ID must not be empty",
            ));
        }
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let entry = inner
            .consumer_health
            .entry(consumer_id.to_owned())
            .or_insert_with(|| ConsumerHealthRecord {
                state: ConsumerHealthState::Healthy,
                strikes: 0,
            });
        entry.strikes = entry.strikes.saturating_add(match signal {
            ConsumerHealthSignal::MissedAck | ConsumerHealthSignal::MissedLease => 1,
        });
        entry.state = match entry.strikes {
            0 => ConsumerHealthState::Healthy,
            1 => ConsumerHealthState::Backpressured,
            2 => ConsumerHealthState::Retrying,
            _ => ConsumerHealthState::Failed,
        };
        Ok(entry.state)
    }

    pub fn consumer_health_state(&self, consumer_id: &str) -> ConsumerHealthState {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .consumer_health
            .get(consumer_id)
            .map(|record| record.state)
            .unwrap_or(ConsumerHealthState::Healthy)
    }

    pub fn clear_consumer_backpressure(&self, consumer_id: &str) {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if let Some(record) = inner.consumer_health.get_mut(consumer_id) {
            record.state = ConsumerHealthState::Healthy;
            record.strikes = 0;
        }
    }

    pub fn lease_audit_events(&self) -> Vec<LeaseAuditEvent> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .lease_audit
            .clone()
    }

    pub fn register_send(
        &self,
        request: IdempotencyRequest,
        _now: SystemTime,
    ) -> Result<IdempotencyDecision, IdempotencyError> {
        request.validate()?;
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let scope = (request.sender_agent.clone(), request.key.clone());
        if let Some(record) = inner.idempotency.get(&scope) {
            if record.subject != request.subject {
                return Ok(IdempotencyDecision::Conflict {
                    reason: IdempotencyConflictReason::SubjectMismatch,
                });
            }
            if record.payload_fingerprint != request.payload_fingerprint {
                return Ok(IdempotencyDecision::Conflict {
                    reason: IdempotencyConflictReason::PayloadFingerprintMismatch,
                });
            }
            if record.operation_kind != request.operation_kind {
                return Ok(IdempotencyDecision::Conflict {
                    reason: IdempotencyConflictReason::OperationKindMismatch,
                });
            }
            return Ok(match &record.state {
                IdempotencyState::Pending => IdempotencyDecision::Unknown {
                    correlation_id: record.correlation_id.clone(),
                    trace_context: record.trace_context.clone(),
                },
                IdempotencyState::Committed(outcome) => IdempotencyDecision::Deduplicated {
                    original_outcome: outcome.clone(),
                    correlation_id: record.correlation_id.clone(),
                    trace_context: record.trace_context.clone(),
                },
            });
        }
        inner.idempotency.insert(
            scope,
            IdempotencyRecord {
                subject: request.subject,
                payload_fingerprint: request.payload_fingerprint,
                operation_kind: request.operation_kind,
                correlation_id: request.correlation_id,
                trace_context: request.trace_context,
                state: IdempotencyState::Pending,
            },
        );
        Ok(IdempotencyDecision::FirstAttempt)
    }

    pub fn open_stream(
        &self,
        registration: StreamRegistration,
    ) -> Result<(), StreamError> {
        registration.validate()?;
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if inner.streams.contains_key(&registration.stream_id) {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                format!("stream '{}' is already open", registration.stream_id),
            ));
        }
        inner
            .stream_correlation_index
            .insert(registration.correlation_id.clone(), registration.stream_id.clone());
        inner.streams.insert(
            registration.stream_id.clone(),
            StreamRecord {
                registration,
                next_sequence: 0,
                outstanding_bytes: 0,
                state: StreamState::Open,
            },
        );
        Ok(())
    }

    pub fn cancel_request(
        &self,
        correlation_id: &str,
        now: SystemTime,
    ) -> Result<CancellationOutcome, BrokerError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        if let Some(pending) = inner.pending_requests.remove(correlation_id) {
            let outcome = CoordinationOutcome::new(
                CoordinationOutcomeKind::Terminal,
                CoordinationStage::Transport,
                "CANCELLED",
                format!("request {correlation_id} cancelled"),
                false,
                true,
                pending.deadline.duration_since(now).map(|_| 0).unwrap_or(0),
            );
            let _ = pending
                .resolution_tx
                .send(RequestResolution::Cancelled {
                    correlation_id: correlation_id.to_owned(),
                    outcome: outcome.clone(),
                });
            inner
                .completed_correlations
                .insert(correlation_id.to_owned());
            inner
                .cancelled_correlations
                .insert(correlation_id.to_owned());
            return Ok(CancellationOutcome::Cancelled(outcome));
        }
        if inner.timed_out_correlations.contains(correlation_id) {
            return Ok(CancellationOutcome::AlreadyTimedOut);
        }
        if inner.completed_correlations.contains(correlation_id) {
            return Ok(CancellationOutcome::AlreadyComplete);
        }
        Ok(CancellationOutcome::NotFound)
    }

    pub fn cancel_stream_by_correlation(
        &self,
        correlation_id: &str,
    ) -> Result<CancellationOutcome, BrokerError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(stream_id) = inner.stream_correlation_index.get(correlation_id).cloned() else {
            return Ok(CancellationOutcome::NotFound);
        };
        let Some(record) = inner.streams.get_mut(&stream_id) else {
            return Ok(CancellationOutcome::NotFound);
        };
        match record.state {
            StreamState::Completed => return Ok(CancellationOutcome::AlreadyComplete),
            StreamState::Aborted => return Ok(CancellationOutcome::AlreadyComplete),
            StreamState::Open => {}
        }
        record.state = StreamState::Aborted;
        let outcome = CoordinationOutcome::new(
            CoordinationOutcomeKind::Terminal,
            CoordinationStage::Transport,
            "CANCELLED",
            format!("stream '{stream_id}' cancelled by correlation {correlation_id}"),
            false,
            true,
            record.next_sequence,
        );
        inner
            .cancelled_correlations
            .insert(correlation_id.to_owned());
        Ok(CancellationOutcome::Cancelled(outcome))
    }

    pub fn was_correlation_cancelled(&self, correlation_id: &str) -> bool {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .cancelled_correlations
            .contains(correlation_id)
    }

    pub fn submit_chunk(
        &self,
        chunk: ChunkSubmission,
    ) -> Result<ChunkSubmissionOutcome, StreamError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(record) = inner.streams.get_mut(&chunk.stream_id) else {
            return Err(StreamError::new(
                StreamErrorCode::StreamUnknown,
                format!("stream '{}' is unknown", chunk.stream_id),
            ));
        };
        if record.state != StreamState::Open {
            return Err(StreamError::new(
                StreamErrorCode::StreamClosed,
                format!(
                    "stream '{}' is in terminal state {:?}",
                    chunk.stream_id, record.state
                ),
            ));
        }
        if chunk.payload.len() > record.registration.max_chunk_size {
            return Err(StreamError::new(
                StreamErrorCode::ChunkPayloadLimit,
                format!(
                    "chunk size {} exceeds max chunk size {}",
                    chunk.payload.len(),
                    record.registration.max_chunk_size
                ),
            ));
        }
        if chunk.sequence != record.next_sequence {
            return Ok(ChunkSubmissionOutcome::SequenceGap {
                expected: record.next_sequence,
                received: chunk.sequence,
            });
        }
        if record.outstanding_bytes + chunk.payload.len() > record.registration.byte_budget {
            return Ok(ChunkSubmissionOutcome::BudgetExhausted {
                outstanding_bytes: record.outstanding_bytes,
                byte_budget: record.registration.byte_budget,
                requested_bytes: chunk.payload.len(),
            });
        }

        let chunk_size = chunk.payload.len();
        record.outstanding_bytes += chunk_size;
        record.next_sequence += 1;
        let sequence = chunk.sequence;
        let outstanding_bytes = record.outstanding_bytes;
        match chunk.finality {
            StreamFinality::Continue => Ok(ChunkSubmissionOutcome::Accepted {
                sequence,
                outstanding_bytes,
                chunk_size,
            }),
            StreamFinality::Final => {
                record.state = StreamState::Completed;
                let total_chunks = record.next_sequence;
                let send_outcome = CoordinationOutcome::acknowledged(
                    format!(
                        "stream '{}' completed after {} chunks",
                        chunk.stream_id, total_chunks
                    ),
                    total_chunks,
                );
                Ok(ChunkSubmissionOutcome::Completed {
                    sequence,
                    total_chunks,
                    send_outcome,
                })
            }
        }
    }

    pub fn acknowledge_consumed(
        &self,
        stream_id: &str,
        bytes: usize,
    ) -> Result<usize, StreamError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(record) = inner.streams.get_mut(stream_id) else {
            return Err(StreamError::new(
                StreamErrorCode::StreamUnknown,
                format!("stream '{stream_id}' is unknown"),
            ));
        };
        if record.outstanding_bytes < bytes {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                format!(
                    "consumed bytes {} exceeds outstanding {} for stream '{stream_id}'",
                    bytes, record.outstanding_bytes
                ),
            ));
        }
        record.outstanding_bytes -= bytes;
        Ok(record.outstanding_bytes)
    }

    pub fn abort_stream(
        &self,
        stream_id: &str,
        reason: StreamTerminationReason,
    ) -> Result<CoordinationOutcome, StreamError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let Some(record) = inner.streams.get_mut(stream_id) else {
            return Err(StreamError::new(
                StreamErrorCode::StreamUnknown,
                format!("stream '{stream_id}' is unknown"),
            ));
        };
        if record.state != StreamState::Open {
            return Err(StreamError::new(
                StreamErrorCode::StreamClosed,
                format!("stream '{stream_id}' is already in terminal state"),
            ));
        }
        record.state = StreamState::Aborted;
        Ok(CoordinationOutcome::failed(
            "STREAM_ABORTED",
            format!(
                "stream '{stream_id}' aborted with reason {}",
                reason.as_str()
            ),
            false,
        ))
    }

    pub fn stream_state(&self, stream_id: &str) -> Option<StreamState> {
        self.inner
            .lock()
            .expect("broker lock is not poisoned")
            .streams
            .get(stream_id)
            .map(|record| record.state)
    }

    pub fn commit_send(
        &self,
        sender_agent: &str,
        key: &str,
        outcome: IdempotencySendOutcome,
    ) -> Result<(), IdempotencyError> {
        let mut inner = self.inner.lock().expect("broker lock is not poisoned");
        let scope = (sender_agent.to_owned(), key.to_owned());
        let Some(record) = inner.idempotency.get_mut(&scope) else {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Unknown,
                format!("idempotency record for sender '{sender_agent}' key '{key}' is unknown"),
            ));
        };
        if matches!(record.state, IdempotencyState::Committed(_)) {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::AlreadyCommitted,
                format!(
                    "idempotency record for sender '{sender_agent}' key '{key}' is already committed"
                ),
            ));
        }
        let coord = match outcome {
            IdempotencySendOutcome::Accepted(coord) => coord,
            IdempotencySendOutcome::Rejected(coord) => coord,
        };
        record.state = IdempotencyState::Committed(coord);
        Ok(())
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
    idempotency: HashMap<(String, String), IdempotencyRecord>,
    streams: HashMap<String, StreamRecord>,
    stream_correlation_index: HashMap<String, String>,
    cancelled_correlations: HashSet<String>,
    queue_bounds: HashMap<String, QueueBoundsConfig>,
    consumer_health: HashMap<String, ConsumerHealthRecord>,
    agent_cards: HashMap<String, AgentCard>,
    capabilities: HashMap<String, AgentCapabilities>,
    capability_change_events: Vec<CapabilityChangeEvent>,
}

#[derive(Debug, Clone, Default)]
struct AgentCapabilities {
    offered: Vec<CapabilityDescriptor>,
    consumed: Vec<CapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCapabilitySummary {
    pub agent: AgentCard,
    pub offered: Vec<CapabilityDescriptor>,
    pub consumed: Vec<CapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityDeclarationOutcome {
    Updated {
        offered: Vec<CapabilityDescriptor>,
        consumed: Vec<CapabilityDescriptor>,
        change_kind: CapabilityChangeKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityChangeKind {
    Initial,
    Changed,
}

impl CapabilityChangeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Changed => "changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityChangeEvent {
    agent_canonical_id: String,
    kind: CapabilityChangeKind,
    offered_count: usize,
    consumed_count: usize,
}

impl CapabilityChangeEvent {
    pub fn agent_canonical_id(&self) -> &str {
        &self.agent_canonical_id
    }

    pub const fn kind(&self) -> CapabilityChangeKind {
        self.kind
    }

    pub const fn offered_count(&self) -> usize {
        self.offered_count
    }

    pub const fn consumed_count(&self) -> usize {
        self.consumed_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityErrorCode {
    AgentNotFound,
}

impl CapabilityErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AgentNotFound => "E_CAPABILITY_AGENT_NOT_FOUND",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityError {
    code: CapabilityErrorCode,
    message: String,
}

impl CapabilityError {
    fn new(code: CapabilityErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> CapabilityErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for CapabilityError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRegistrationOutcome {
    Registered { canonical: AgentCard },
    Compatible { canonical: AgentCard },
    Conflict {
        existing: AgentCard,
        attempted: AgentCard,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueBoundsConfig {
    max_depth: usize,
    drop_policy: QueueDropPolicy,
}

impl QueueBoundsConfig {
    pub const fn new(max_depth: usize, drop_policy: QueueDropPolicy) -> Self {
        Self {
            max_depth,
            drop_policy,
        }
    }

    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }

    pub const fn drop_policy(&self) -> QueueDropPolicy {
        self.drop_policy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueDropPolicy {
    Reject,
    DropOldest,
    DropNewest,
}

impl QueueDropPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::DropOldest => "drop_oldest",
            Self::DropNewest => "drop_newest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackpressureOutcome {
    Accepted,
    Deferred {
        details: BackpressureDetails,
    },
    RejectedBackpressure {
        details: BackpressureDetails,
    },
    DroppedByPolicy {
        policy: QueueDropPolicy,
        details: BackpressureDetails,
        dropped_correlation_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackpressureDetails {
    subject_scope: String,
    queue_bound: usize,
    exceeded_limit: usize,
    retryable: bool,
    suggested_delay: Duration,
    remediation: String,
}

impl BackpressureDetails {
    pub fn subject_scope(&self) -> &str {
        &self.subject_scope
    }

    pub const fn queue_bound(&self) -> usize {
        self.queue_bound
    }

    pub const fn exceeded_limit(&self) -> usize {
        self.exceeded_limit
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    pub const fn suggested_delay(&self) -> Duration {
        self.suggested_delay
    }

    pub fn remediation(&self) -> &str {
        &self.remediation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerHealthState {
    Healthy,
    Backpressured,
    Retrying,
    Failed,
}

impl ConsumerHealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Backpressured => "backpressured",
            Self::Retrying => "retrying",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerHealthSignal {
    MissedAck,
    MissedLease,
}

impl ConsumerHealthSignal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissedAck => "missed_ack",
            Self::MissedLease => "missed_lease",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ConsumerHealthRecord {
    state: ConsumerHealthState,
    strikes: u32,
}

#[derive(Debug, Clone)]
struct StreamRecord {
    registration: StreamRegistration,
    next_sequence: u32,
    outstanding_bytes: usize,
    state: StreamState,
}

pub const MAX_STREAM_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_STREAM_BYTE_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRegistration {
    stream_id: String,
    correlation_id: String,
    sender_agent: String,
    receiver_agent: String,
    max_chunk_size: usize,
    byte_budget: usize,
}

impl StreamRegistration {
    pub fn new(
        stream_id: impl Into<String>,
        correlation_id: impl Into<String>,
        sender_agent: impl Into<String>,
        receiver_agent: impl Into<String>,
        max_chunk_size: usize,
        byte_budget: usize,
    ) -> Self {
        Self {
            stream_id: stream_id.into(),
            correlation_id: correlation_id.into(),
            sender_agent: sender_agent.into(),
            receiver_agent: receiver_agent.into(),
            max_chunk_size,
            byte_budget,
        }
    }

    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn sender_agent(&self) -> &str {
        &self.sender_agent
    }

    pub fn receiver_agent(&self) -> &str {
        &self.receiver_agent
    }

    pub const fn max_chunk_size(&self) -> usize {
        self.max_chunk_size
    }

    pub const fn byte_budget(&self) -> usize {
        self.byte_budget
    }

    fn validate(&self) -> Result<(), StreamError> {
        if self.stream_id.trim().is_empty() {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                "stream ID must not be empty",
            ));
        }
        if self.correlation_id.trim().is_empty() {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                "correlation ID must not be empty",
            ));
        }
        if self.sender_agent.trim().is_empty() || self.receiver_agent.trim().is_empty() {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                "stream sender and receiver agents must not be empty",
            ));
        }
        if self.max_chunk_size == 0 || self.max_chunk_size > MAX_STREAM_CHUNK_BYTES {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                format!(
                    "max chunk size {} must be in (0, {MAX_STREAM_CHUNK_BYTES}]",
                    self.max_chunk_size
                ),
            ));
        }
        if self.byte_budget == 0 || self.byte_budget > MAX_STREAM_BYTE_BUDGET {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                format!(
                    "byte budget {} must be in (0, {MAX_STREAM_BYTE_BUDGET}]",
                    self.byte_budget
                ),
            ));
        }
        if self.byte_budget < self.max_chunk_size {
            return Err(StreamError::new(
                StreamErrorCode::StreamValidation,
                "byte budget must be at least one chunk",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFinality {
    Continue,
    Final,
}

impl StreamFinality {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Final => "final",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkSubmission {
    stream_id: String,
    sequence: u32,
    payload: Vec<u8>,
    finality: StreamFinality,
}

impl ChunkSubmission {
    pub fn new(
        stream_id: impl Into<String>,
        sequence: u32,
        payload: Vec<u8>,
        finality: StreamFinality,
    ) -> Self {
        Self {
            stream_id: stream_id.into(),
            sequence,
            payload,
            finality,
        }
    }

    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub const fn finality(&self) -> StreamFinality {
        self.finality
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkSubmissionOutcome {
    Accepted {
        sequence: u32,
        chunk_size: usize,
        outstanding_bytes: usize,
    },
    Completed {
        sequence: u32,
        total_chunks: u32,
        send_outcome: CoordinationOutcome,
    },
    BudgetExhausted {
        outstanding_bytes: usize,
        byte_budget: usize,
        requested_bytes: usize,
    },
    SequenceGap {
        expected: u32,
        received: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Open,
    Completed,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationOutcome {
    Cancelled(CoordinationOutcome),
    AlreadyComplete,
    AlreadyTimedOut,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTerminationReason {
    SenderCancelled,
    ReceiverFailure,
    DaemonDisconnect,
}

impl StreamTerminationReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SenderCancelled => "sender_cancelled",
            Self::ReceiverFailure => "receiver_failure",
            Self::DaemonDisconnect => "daemon_disconnect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorCode {
    StreamUnknown,
    StreamClosed,
    ChunkPayloadLimit,
    StreamValidation,
}

impl StreamErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StreamUnknown => "E_STREAM_UNKNOWN",
            Self::StreamClosed => "E_STREAM_CLOSED",
            Self::ChunkPayloadLimit => "E_CHUNK_PAYLOAD_LIMIT",
            Self::StreamValidation => "E_STREAM_VALIDATION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamError {
    code: StreamErrorCode,
    message: String,
}

impl StreamError {
    fn new(code: StreamErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> StreamErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for StreamError {}

#[derive(Debug, Clone)]
struct IdempotencyRecord {
    subject: String,
    payload_fingerprint: String,
    operation_kind: String,
    correlation_id: String,
    trace_context: Option<String>,
    state: IdempotencyState,
}

#[derive(Debug, Clone)]
enum IdempotencyState {
    Pending,
    Committed(CoordinationOutcome),
}

pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRequest {
    sender_agent: String,
    key: String,
    subject: String,
    payload_fingerprint: String,
    operation_kind: String,
    correlation_id: String,
    trace_context: Option<String>,
    timeout: Option<Duration>,
}

impl IdempotencyRequest {
    pub fn new(
        sender_agent: impl Into<String>,
        key: impl Into<String>,
        subject: impl Into<String>,
        payload_fingerprint: impl Into<String>,
        operation_kind: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            sender_agent: sender_agent.into(),
            key: key.into(),
            subject: subject.into(),
            payload_fingerprint: payload_fingerprint.into(),
            operation_kind: operation_kind.into(),
            correlation_id: correlation_id.into(),
            trace_context: None,
            timeout: None,
        }
    }

    pub fn with_trace_context(mut self, trace_context: impl Into<String>) -> Self {
        self.trace_context = Some(trace_context.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn sender_agent(&self) -> &str {
        &self.sender_agent
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn payload_fingerprint(&self) -> &str {
        &self.payload_fingerprint
    }

    pub fn operation_kind(&self) -> &str {
        &self.operation_kind
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn trace_context(&self) -> Option<&str> {
        self.trace_context.as_deref()
    }

    pub const fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    fn validate(&self) -> Result<(), IdempotencyError> {
        if self.sender_agent.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "sender agent must not be empty",
            ));
        }
        if self.key.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "idempotency key must not be empty",
            ));
        }
        if self.key.len() > MAX_IDEMPOTENCY_KEY_BYTES {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                format!(
                    "idempotency key is {} bytes; maximum is {MAX_IDEMPOTENCY_KEY_BYTES}",
                    self.key.len()
                ),
            ));
        }
        if self.subject.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "subject must not be empty",
            ));
        }
        if self.payload_fingerprint.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "payload fingerprint must not be empty",
            ));
        }
        if self.operation_kind.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "operation kind must not be empty",
            ));
        }
        if self.correlation_id.trim().is_empty() {
            return Err(IdempotencyError::new(
                IdempotencyErrorCode::Validation,
                "correlation ID must not be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyDecision {
    FirstAttempt,
    Deduplicated {
        original_outcome: CoordinationOutcome,
        correlation_id: String,
        trace_context: Option<String>,
    },
    Conflict {
        reason: IdempotencyConflictReason,
    },
    Unknown {
        correlation_id: String,
        trace_context: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyConflictReason {
    SubjectMismatch,
    PayloadFingerprintMismatch,
    OperationKindMismatch,
}

impl IdempotencyConflictReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubjectMismatch => "subject_mismatch",
            Self::PayloadFingerprintMismatch => "payload_fingerprint_mismatch",
            Self::OperationKindMismatch => "operation_kind_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencySendOutcome {
    Accepted(CoordinationOutcome),
    Rejected(CoordinationOutcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyErrorCode {
    Validation,
    Unknown,
    AlreadyCommitted,
}

impl IdempotencyErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "E_IDEMPOTENCY_VALIDATION",
            Self::Unknown => "E_IDEMPOTENCY_UNKNOWN",
            Self::AlreadyCommitted => "E_IDEMPOTENCY_ALREADY_COMMITTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyError {
    code: IdempotencyErrorCode,
    message: String,
}

impl IdempotencyError {
    fn new(code: IdempotencyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> IdempotencyErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for IdempotencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for IdempotencyError {}

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
    Cancelled {
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
