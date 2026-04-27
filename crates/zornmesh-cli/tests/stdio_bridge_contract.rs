use zornmesh_broker::{Broker, PeerCredentials, SocketTrustPolicy};
use zornmesh_cli::{
    BridgeMessage, BridgeResponse, BridgeState, MCP_BRIDGE_PROTOCOL_VERSION, StdioBridge,
    StdioBridgeError, StdioBridgeErrorCode,
};

const OWNER_UID: u32 = 1000;
const OWNER_GID: u32 = 1000;

fn safe_policy() -> SocketTrustPolicy {
    SocketTrustPolicy::new(OWNER_UID, OWNER_GID, 0o600)
}

fn make_bridge(broker: &Broker) -> StdioBridge {
    StdioBridge::new(
        broker.clone(),
        "agent.local/mcp-host",
        "MCP Host",
        PeerCredentials::new(OWNER_UID, OWNER_GID, 4242),
        safe_policy(),
    )
}

#[test]
fn initial_state_is_pending_and_requires_initialize_first() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    assert_eq!(bridge.state(), BridgeState::Pending);

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/list".to_owned(),
        params: String::new(),
    });
    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::OutOfSequence);
        }
        other => panic!("expected OutOfSequence error, got {other:?}"),
    }
    assert_eq!(bridge.state(), BridgeState::Pending);
}

#[test]
fn supported_initialize_transitions_to_initialized_and_registers_agent() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);

    let response = bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });
    match response {
        BridgeResponse::InitializeAck { protocol_version } => {
            assert_eq!(protocol_version, MCP_BRIDGE_PROTOCOL_VERSION);
        }
        other => panic!("expected InitializeAck, got {other:?}"),
    }
    assert!(matches!(bridge.state(), BridgeState::Initialized { .. }));

    // Agent should be registered in broker.
    let card = broker.lookup_agent_card("agent.local/mcp-host");
    assert!(card.is_some());
}

#[test]
fn unsupported_protocol_version_is_rejected_without_creating_identity() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    let response = bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: "9999-99-99".to_owned(),
    });
    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::UnsupportedProtocolVersion);
        }
        other => panic!("expected UnsupportedProtocolVersion, got {other:?}"),
    }
    assert_eq!(bridge.state(), BridgeState::Pending);
    assert!(broker.lookup_agent_card("agent.local/mcp-host").is_none());
}

#[test]
fn duplicate_initialize_after_success_returns_already_initialized_error() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });
    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::AlreadyInitialized);
        }
        other => panic!("expected AlreadyInitialized, got {other:?}"),
    }
}

#[test]
fn supported_request_after_initialize_maps_to_internal_operation_with_correlation_id() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "ping".to_owned(),
        params: "{}".to_owned(),
    });
    match response {
        BridgeResponse::Mapped {
            correlation_id,
            internal_operation,
            ..
        } => {
            assert!(!correlation_id.is_empty());
            assert_eq!(internal_operation, "ping");
        }
        other => panic!("expected Mapped, got {other:?}"),
    }
}

#[test]
fn host_close_transitions_to_closed_and_cleans_up_session_state() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });
    let agent_id = "agent.local/mcp-host";
    assert!(!broker.active_sessions(agent_id).is_empty());

    bridge.handle_message(BridgeMessage::HostClosed);
    assert_eq!(bridge.state(), BridgeState::Closed);
    assert!(broker.active_sessions(agent_id).is_empty());
}

#[test]
fn malformed_initialize_with_empty_protocol_version_returns_validation_error() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    let response = bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: String::new(),
    });
    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::MalformedInitialize);
        }
        other => panic!("expected MalformedInitialize, got {other:?}"),
    }
}

#[test]
fn requests_after_close_are_rejected_with_closed_error() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });
    bridge.handle_message(BridgeMessage::HostClosed);

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "ping".to_owned(),
        params: "{}".to_owned(),
    });
    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::Closed);
        }
        other => panic!("expected Closed, got {other:?}"),
    }
}

fn _force_use_error_type(_e: &StdioBridgeError) {
    // marker — keep StdioBridgeError type in scope for visibility.
}

#[test]
fn fixture_pins_stdio_bridge_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "stdio_bridge|state|pending",
        "stdio_bridge|state|initialized",
        "stdio_bridge|state|closed",
        "stdio_bridge|error|E_BRIDGE_OUT_OF_SEQUENCE",
        "stdio_bridge|error|E_BRIDGE_ALREADY_INITIALIZED",
        "stdio_bridge|error|E_BRIDGE_UNSUPPORTED_PROTOCOL",
        "stdio_bridge|error|E_BRIDGE_MALFORMED_INITIALIZE",
        "stdio_bridge|error|E_BRIDGE_CLOSED",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
