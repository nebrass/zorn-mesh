use zornmesh_broker::{
    AgentRegistrationOutcome, Broker,
};
use zornmesh_core::{AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardInput};

fn input(stable_id: &str, display: &str, transport: &str) -> AgentCardInput {
    AgentCardInput {
        profile_version: AGENT_CARD_PROFILE_VERSION.to_owned(),
        stable_id: stable_id.to_owned(),
        display_name: display.to_owned(),
        transport: transport.to_owned(),
        source: "agent.local/source".to_owned(),
    }
}

#[test]
fn first_registration_returns_registered_canonical_reference() {
    let broker = Broker::new();
    let card = AgentCard::from_input(input("agent.local/Alpha", "Alpha", "unix")).unwrap();

    let outcome = broker.register_agent_card(card.clone()).unwrap();
    match outcome {
        AgentRegistrationOutcome::Registered { canonical } => {
            assert_eq!(canonical.canonical_stable_id(), "agent.local/alpha");
        }
        other => panic!("expected Registered, got {other:?}"),
    }
}

#[test]
fn duplicate_compatible_registration_resolves_to_same_canonical_reference() {
    let broker = Broker::new();
    let first = AgentCard::from_input(input("agent.local/Alpha", "Alpha", "unix")).unwrap();
    let case_variant =
        AgentCard::from_input(input("agent.local/ALPHA", "Alpha (renamed)", "unix")).unwrap();

    broker.register_agent_card(first.clone()).unwrap();
    let outcome = broker.register_agent_card(case_variant).unwrap();
    match outcome {
        AgentRegistrationOutcome::Compatible { canonical } => {
            assert_eq!(canonical.canonical_stable_id(), "agent.local/alpha");
        }
        other => panic!("expected Compatible, got {other:?}"),
    }
}

#[test]
fn duplicate_incompatible_registration_returns_conflict() {
    let broker = Broker::new();
    let first = AgentCard::from_input(input("agent.local/Alpha", "Alpha", "unix")).unwrap();
    let mismatch = AgentCard::from_input(input("agent.local/Alpha", "Alpha", "tcp")).unwrap();

    broker.register_agent_card(first).unwrap();
    let outcome = broker.register_agent_card(mismatch).unwrap();
    assert!(matches!(outcome, AgentRegistrationOutcome::Conflict { .. }));
}

#[test]
fn registration_lookup_by_canonical_id_returns_stored_card() {
    let broker = Broker::new();
    let card = AgentCard::from_input(input("agent.local/Alpha", "Alpha", "unix")).unwrap();
    broker.register_agent_card(card.clone()).unwrap();

    let stored = broker.lookup_agent_card("agent.local/alpha");
    assert!(stored.is_some());
    let stored = stored.unwrap();
    assert_eq!(stored.canonical_stable_id(), card.canonical_stable_id());
}
