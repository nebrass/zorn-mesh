use zornmesh_broker::{
    AgentPresenceState, AgentRegistrationOutcome, Broker, ConnectionAcceptanceOutcome,
    PeerCredentials, SocketTrustErrorCode, SocketTrustPolicy,
};
use zornmesh_core::{AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardInput};

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
        AgentRegistrationOutcome::Conflict { .. } => panic!("conflict in test setup"),
    }
}

fn safe_policy() -> SocketTrustPolicy {
    SocketTrustPolicy::new(OWNER_UID, OWNER_GID, 0o600)
}

#[test]
fn matching_peer_uid_with_safe_socket_mode_is_accepted_and_marks_agent_connected() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/alpha");
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, 4242);

    let outcome = broker
        .accept_connection(agent.canonical_stable_id(), creds, safe_policy(), 0o600)
        .expect("connection accepted");
    match outcome {
        ConnectionAcceptanceOutcome::Accepted { credentials } => {
            assert_eq!(credentials.uid(), OWNER_UID);
            assert_eq!(credentials.pid(), 4242);
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Connected
    );
}

#[test]
fn mismatched_peer_uid_is_rejected_without_creating_presence_state() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/beta");
    let foreign = PeerCredentials::new(OWNER_UID + 1, OWNER_GID, 1234);

    let outcome = broker
        .accept_connection(agent.canonical_stable_id(), foreign, safe_policy(), 0o600)
        .expect("rejection outcome returns");
    match outcome {
        ConnectionAcceptanceOutcome::Rejected { code, .. } => {
            assert_eq!(code, SocketTrustErrorCode::ForeignUid);
        }
        other => panic!("expected Rejected/ForeignUid, got {other:?}"),
    }
    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Disconnected
    );
}

#[test]
fn unsafe_socket_mode_with_group_or_other_bits_is_rejected() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/gamma");
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, 9999);

    let group_writable = broker
        .accept_connection(agent.canonical_stable_id(), creds.clone(), safe_policy(), 0o620)
        .unwrap();
    match group_writable {
        ConnectionAcceptanceOutcome::Rejected { code, .. } => {
            assert_eq!(code, SocketTrustErrorCode::UnsafeMode);
        }
        other => panic!("expected Rejected/UnsafeMode, got {other:?}"),
    }

    let other_readable = broker
        .accept_connection(agent.canonical_stable_id(), creds, safe_policy(), 0o604)
        .unwrap();
    match other_readable {
        ConnectionAcceptanceOutcome::Rejected { code, .. } => {
            assert_eq!(code, SocketTrustErrorCode::UnsafeMode);
        }
        other => panic!("expected Rejected/UnsafeMode, got {other:?}"),
    }
}

#[test]
fn connection_for_unknown_agent_is_rejected() {
    let broker = Broker::new();
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, 4242);
    let outcome = broker
        .accept_connection("agent.local/missing", creds, safe_policy(), 0o600)
        .unwrap();
    match outcome {
        ConnectionAcceptanceOutcome::Rejected { code, .. } => {
            assert_eq!(code, SocketTrustErrorCode::UnknownAgent);
        }
        other => panic!("expected Rejected/UnknownAgent, got {other:?}"),
    }
}

#[test]
fn record_disconnect_transitions_state_and_subsequent_routing_can_observe_disconnected() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/delta");
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, 4242);
    broker
        .accept_connection(agent.canonical_stable_id(), creds, safe_policy(), 0o600)
        .unwrap();
    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Connected
    );

    broker.record_disconnect(agent.canonical_stable_id());
    assert_eq!(
        broker.agent_presence_state(agent.canonical_stable_id()),
        AgentPresenceState::Disconnected
    );

    let presence_events = broker.agent_presence_events();
    assert!(presence_events
        .iter()
        .any(|e| e.agent_canonical_id() == agent.canonical_stable_id()
            && e.state() == AgentPresenceState::Disconnected));
}

#[test]
fn unsupported_socket_form_returns_remediation_text() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/echo");
    let creds = PeerCredentials::new(OWNER_UID, OWNER_GID, 4242);

    // Mark the agent's transport as in_process; current policy only accepts unix.
    let outcome = broker
        .accept_connection_with_transport(
            agent.canonical_stable_id(),
            creds,
            safe_policy(),
            0o600,
            "tcp",
        )
        .unwrap();
    match outcome {
        ConnectionAcceptanceOutcome::Rejected { code, remediation } => {
            assert_eq!(code, SocketTrustErrorCode::UnsupportedSocketForm);
            assert!(remediation.contains("unix") || remediation.contains("Unix"));
        }
        other => panic!("expected Rejected/UnsupportedSocketForm, got {other:?}"),
    }
}

#[test]
fn fixture_pins_socket_trust_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "socket_trust|outcome|accepted",
        "socket_trust|outcome|rejected",
        "socket_trust|error|E_SOCKET_FOREIGN_UID",
        "socket_trust|error|E_SOCKET_UNSAFE_MODE",
        "socket_trust|error|E_SOCKET_UNKNOWN_AGENT",
        "socket_trust|error|E_SOCKET_UNSUPPORTED_FORM",
        "agent_presence|state|connected",
        "agent_presence|state|disconnected",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
