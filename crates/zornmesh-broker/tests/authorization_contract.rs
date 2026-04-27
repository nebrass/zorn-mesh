use zornmesh_broker::{
    AgentRegistrationOutcome, AuthorizationDecision, AuthorizationDenialReason, Broker,
    CapabilityDeclarationOutcome, HighPrivilegeAllowlistEntry,
};
use zornmesh_core::{
    AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardInput, CapabilityDescriptor,
    CapabilityDirection, CapabilitySchemaDialect,
};

fn agent_input(stable_id: &str) -> AgentCardInput {
    AgentCardInput {
        profile_version: AGENT_CARD_PROFILE_VERSION.to_owned(),
        stable_id: stable_id.to_owned(),
        display_name: "Test Agent".to_owned(),
        transport: "unix".to_owned(),
        source: "agent.local/source".to_owned(),
    }
}

fn cap(id: &str, version: &str, direction: CapabilityDirection) -> CapabilityDescriptor {
    CapabilityDescriptor::builder(id, version, direction)
        .with_summary("test capability")
        .with_schema_ref(CapabilitySchemaDialect::TypeBox, format!("{id}.{version}"))
        .build()
        .unwrap()
}

fn register_agent(broker: &Broker, stable_id: &str) -> AgentCard {
    let card = AgentCard::from_input(agent_input(stable_id)).unwrap();
    let outcome = broker.register_agent_card(card.clone()).unwrap();
    match outcome {
        AgentRegistrationOutcome::Registered { canonical }
        | AgentRegistrationOutcome::Compatible { canonical } => canonical,
        AgentRegistrationOutcome::Conflict { .. } => panic!("conflict"),
    }
}

#[test]
fn high_privilege_capability_without_allowlist_is_denied_at_declaration() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let agent = register_agent(&broker, "agent.local/alpha");

    let outcome = broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![
                cap("admin.shutdown", "v1", CapabilityDirection::Offered),
                cap("compute.run", "v1", CapabilityDirection::Offered),
            ],
        )
        .unwrap();

    let CapabilityDeclarationOutcome::Updated {
        offered,
        consumed,
        denied,
        ..
    } = outcome;
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].capability_id(), "compute.run");
    assert_eq!(consumed.len(), 0);
    assert_eq!(denied.len(), 1);
    assert_eq!(denied[0].descriptor().capability_id(), "admin.shutdown");
    assert_eq!(
        denied[0].reason(),
        AuthorizationDenialReason::NotAllowlisted
    );
}

#[test]
fn allowlisted_high_privilege_capability_is_accepted_and_emits_authorization_event() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let agent = register_agent(&broker, "agent.local/alpha");

    broker.allowlist_high_privilege(HighPrivilegeAllowlistEntry::new(
        agent.canonical_stable_id(),
        "admin.shutdown",
        "v1",
    ));

    let outcome = broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![cap("admin.shutdown", "v1", CapabilityDirection::Offered)],
        )
        .unwrap();
    let CapabilityDeclarationOutcome::Updated {
        offered, denied, ..
    } = outcome;
    assert_eq!(offered.len(), 1);
    assert!(denied.is_empty());

    let events = broker.authorization_events();
    assert!(events.iter().any(
        |event| event.agent_canonical_id() == agent.canonical_stable_id()
            && event.capability_id() == "admin.shutdown"
            && matches!(event.decision(), AuthorizationDecision::Allowed)
    ));
}

#[test]
fn invocation_of_high_privilege_capability_is_denied_without_allowlist() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let agent = register_agent(&broker, "agent.local/alpha");

    let decision = broker.authorize_invocation(agent.canonical_stable_id(), "admin.shutdown", "v1");
    match decision {
        AuthorizationDecision::Denied { reason } => {
            assert_eq!(reason, AuthorizationDenialReason::NotAllowlisted);
        }
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[test]
fn invocation_of_unmarked_capability_is_allowed_by_default() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/alpha");
    let decision = broker.authorize_invocation(agent.canonical_stable_id(), "compute.run", "v1");
    assert!(matches!(decision, AuthorizationDecision::Allowed));
}

#[test]
fn invocation_after_allowlist_is_allowed_and_recorded() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let agent = register_agent(&broker, "agent.local/alpha");
    broker.allowlist_high_privilege(HighPrivilegeAllowlistEntry::new(
        agent.canonical_stable_id(),
        "admin.shutdown",
        "v1",
    ));

    let decision = broker.authorize_invocation(agent.canonical_stable_id(), "admin.shutdown", "v1");
    assert!(matches!(decision, AuthorizationDecision::Allowed));
    assert!(!broker.authorization_events().is_empty());
}

#[test]
fn revocation_removes_capability_for_future_dispatch_and_logs_audit_event() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let agent = register_agent(&broker, "agent.local/alpha");
    broker.allowlist_high_privilege(HighPrivilegeAllowlistEntry::new(
        agent.canonical_stable_id(),
        "admin.shutdown",
        "v1",
    ));
    broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![cap("admin.shutdown", "v1", CapabilityDirection::Offered)],
        )
        .unwrap();

    broker.revoke_high_privilege(agent.canonical_stable_id(), "admin.shutdown", "v1");

    let decision = broker.authorize_invocation(agent.canonical_stable_id(), "admin.shutdown", "v1");
    assert!(matches!(
        decision,
        AuthorizationDecision::Denied {
            reason: AuthorizationDenialReason::NotAllowlisted
        }
    ));

    // Revocation event recorded.
    let revocation_events: Vec<_> = broker
        .authorization_events()
        .into_iter()
        .filter(|e| matches!(e.decision(), AuthorizationDecision::Revoked))
        .collect();
    assert_eq!(revocation_events.len(), 1);
    assert_eq!(revocation_events[0].capability_id(), "admin.shutdown");
}

#[test]
fn fixture_pins_authorization_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "authorization|decision|allowed",
        "authorization|decision|denied",
        "authorization|decision|revoked",
        "authorization|reason|not_allowlisted",
        "authorization|reason|capability_revoked",
        "authorization|error|E_AUTHORIZATION_DENIED",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
