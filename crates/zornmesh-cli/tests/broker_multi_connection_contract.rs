use zornmesh_cli::broker::{
    AgentPresenceState, AgentRegistrationOutcome, Broker, ConnectionAcceptanceOutcome,
    PeerCredentials, SocketTrustPolicy,
};
use zornmesh_cli::core::{AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardInput};

const OWNER_UID: u32 = 1000;
const OWNER_GID: u32 = 1000;

fn agent_input(stable_id: &str) -> AgentCardInput {
    AgentCardInput {
        profile_version: AGENT_CARD_PROFILE_VERSION.to_owned(),
        stable_id: stable_id.to_owned(),
        display_name: "Test Agent".to_owned(),
        transport: "unix".to_owned(),
        source: "agent.local/source".to_owned(),
    }
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

fn safe_policy() -> SocketTrustPolicy {
    SocketTrustPolicy::new(OWNER_UID, OWNER_GID, 0o600)
}

fn accept(broker: &Broker, agent_id: &str, pid: u32) -> ConnectionAcceptanceOutcome {
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, pid);
    broker
        .accept_connection(agent_id, creds, safe_policy(), 0o600)
        .expect("accept_connection returns")
}

#[test]
fn multiple_connections_resolve_to_one_canonical_agent_with_multiple_sessions() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/multi");

    let s1 = accept(&broker, agent.canonical_stable_id(), 1001);
    let s2 = accept(&broker, agent.canonical_stable_id(), 1002);
    let session_id_1 = match s1 {
        ConnectionAcceptanceOutcome::Accepted { .. } => broker
            .active_sessions(agent.canonical_stable_id())
            .first()
            .unwrap()
            .session_id()
            .to_owned(),
        other => panic!("expected Accepted, got {other:?}"),
    };
    assert!(matches!(s2, ConnectionAcceptanceOutcome::Accepted { .. }));

    let sessions = broker.active_sessions(agent.canonical_stable_id());
    assert_eq!(sessions.len(), 2);
    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Connected
    );
    // Routing target is the earliest-acquired session.
    let routing_target = broker
        .routing_session(agent.canonical_stable_id())
        .expect("routing target available");
    assert_eq!(routing_target.session_id(), session_id_1);
}

#[test]
fn disconnect_one_session_keeps_agent_connected_when_another_session_remains() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/multi");
    accept(&broker, agent.canonical_stable_id(), 1001);
    accept(&broker, agent.canonical_stable_id(), 1002);

    let sessions = broker.active_sessions(agent.canonical_stable_id());
    let first_session = sessions[0].session_id().to_owned();
    broker.record_session_disconnect(agent.canonical_stable_id(), &first_session);

    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Connected
    );
    assert_eq!(broker.active_sessions(agent.canonical_stable_id()).len(), 1);
    // Routing now selects the remaining session.
    let routing = broker
        .routing_session(agent.canonical_stable_id())
        .expect("routing still available");
    assert_ne!(routing.session_id(), first_session);
}

#[test]
fn disconnecting_last_session_marks_agent_disconnected() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/single");
    accept(&broker, agent.canonical_stable_id(), 1001);

    let sessions = broker.active_sessions(agent.canonical_stable_id());
    let only_session = sessions[0].session_id().to_owned();
    broker.record_session_disconnect(agent.canonical_stable_id(), &only_session);

    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Disconnected
    );
    assert_eq!(broker.active_sessions(agent.canonical_stable_id()).len(), 0);
    assert!(
        broker
            .routing_session(agent.canonical_stable_id())
            .is_none()
    );
}

#[test]
fn disconnect_for_unknown_session_is_idempotent_no_op() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/idempotent");
    accept(&broker, agent.canonical_stable_id(), 1001);

    broker.record_session_disconnect(agent.canonical_stable_id(), "missing-session");
    assert_eq!(broker.active_sessions(agent.canonical_stable_id()).len(), 1);
}

#[test]
fn each_session_retains_raw_peer_metadata_for_audit() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/audit");
    accept(&broker, agent.canonical_stable_id(), 1001);
    accept(&broker, agent.canonical_stable_id(), 1002);

    let sessions = broker.active_sessions(agent.canonical_stable_id());
    let pids: Vec<u32> = sessions.iter().map(|s| s.credentials().pid()).collect();
    assert!(pids.contains(&1001));
    assert!(pids.contains(&1002));
}

#[test]
fn fixture_pins_multi_connection_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "session|routing|earliest_session_wins",
        "session|disconnect|partial_keeps_present",
        "session|disconnect|last_marks_disconnected",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
