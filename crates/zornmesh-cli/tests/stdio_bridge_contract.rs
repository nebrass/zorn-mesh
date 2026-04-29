use zornmesh_broker::{Broker, PeerCredentials, SocketTrustPolicy};
use zornmesh_cli::{
    BridgeMessage, BridgeResponse, BridgeState, MCP_BRIDGE_PROTOCOL_VERSION, StdioBridge,
    StdioBridgeError, StdioBridgeErrorCode,
};
use zornmesh_core::REDACTION_MARKER;

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
fn supported_request_mapping_preserves_trace_and_capability_metadata_with_redaction() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/call".to_owned(),
        params: r#"{"correlation_id":"corr-1","trace_id":"trace-1","capability_id":"compute.run","capability_version":"v1","password":"hunter2"}"#.to_owned(),
    });

    match response {
        BridgeResponse::Mapped {
            correlation_id,
            trace_id,
            capability_id,
            capability_version,
            safe_params,
            ..
        } => {
            assert_eq!(correlation_id, "corr-1");
            assert_eq!(trace_id.as_deref(), Some("trace-1"));
            assert_eq!(capability_id.as_deref(), Some("compute.run"));
            assert_eq!(capability_version.as_deref(), Some("v1"));
            assert!(!safe_params.contains("hunter2"));
            assert!(safe_params.contains(REDACTION_MARKER));
        }
        other => panic!("expected Mapped, got {other:?}"),
    }
}

#[test]
fn tools_list_after_initialize_exposes_only_baseline_representable_tools() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/list".to_owned(),
        params: "{}".to_owned(),
    });

    match response {
        BridgeResponse::ToolList { tools } => {
            let names: Vec<_> = tools.iter().map(|tool| tool.name()).collect();
            assert_eq!(names, vec!["zornmesh.call_capability"]);
            assert!(
                tools[0].description().contains("baseline MCP"),
                "tool description must document the baseline MCP limit"
            );
            assert!(
                !names.iter().any(|name| name.contains("stream")),
                "streaming-only mesh capabilities must not be exposed as baseline MCP tools"
            );
        }
        other => panic!("expected ToolList, got {other:?}"),
    }
}

#[test]
fn unsupported_streaming_capability_returns_named_result_with_safe_remediation() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/call".to_owned(),
        params:
            r#"{"capability_id":"stream.tokens","requires_streaming":true,"secret":"do-not-leak"}"#
                .to_owned(),
    });

    match response {
        BridgeResponse::UnsupportedCapability {
            code,
            capability_id,
            reason,
            remediation,
            safe_params,
            ..
        } => {
            assert_eq!(code, StdioBridgeErrorCode::UnsupportedCapability.as_str());
            assert_eq!(capability_id.as_deref(), Some("stream.tokens"));
            assert!(reason.contains("streaming"));
            assert!(remediation.contains("zornmesh CLI"));
            assert!(safe_params.contains(REDACTION_MARKER));
            assert!(!safe_params.contains("do-not-leak"));
        }
        other => panic!("expected UnsupportedCapability, got {other:?}"),
    }
}

#[test]
fn delivery_ack_partial_mapping_refuses_explicitly() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/call".to_owned(),
        params: r#"{"capability_id":"queue.publish","requires_delivery_ack":true}"#.to_owned(),
    });

    match response {
        BridgeResponse::UnsupportedCapability { reason, .. } => {
            assert!(reason.contains("delivery_ack"));
        }
        other => panic!("expected UnsupportedCapability, got {other:?}"),
    }
}

#[test]
fn required_trace_context_partial_mapping_refuses_instead_of_silently_degrading() {
    let broker = Broker::new();
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/call".to_owned(),
        params: r#"{"capability_id":"trace.forward","requires_trace_context":true,"traceparent":"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"}"#.to_owned(),
    });

    match response {
        BridgeResponse::UnsupportedCapability {
            reason,
            safe_params,
            ..
        } => {
            assert!(reason.contains("trace_context"));
            assert!(safe_params.contains("traceparent"));
        }
        other => panic!("expected UnsupportedCapability, got {other:?}"),
    }
}

#[test]
fn policy_denied_capability_call_returns_stable_error_without_dispatch() {
    let broker = Broker::new();
    broker.mark_capability_high_privilege("admin.shutdown", "v1");
    let mut bridge = make_bridge(&broker);
    bridge.handle_message(BridgeMessage::Initialize {
        protocol_version: MCP_BRIDGE_PROTOCOL_VERSION.to_owned(),
    });

    let response = bridge.handle_message(BridgeMessage::Request {
        method: "tools/call".to_owned(),
        params: r#"{"capability_id":"admin.shutdown","capability_version":"v1"}"#.to_owned(),
    });

    match response {
        BridgeResponse::Error(err) => {
            assert_eq!(err.code(), StdioBridgeErrorCode::PolicyDenied);
            assert!(err.safe_message().contains("not_allowlisted"));
        }
        other => panic!("expected PolicyDenied error, got {other:?}"),
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
        "stdio_bridge|result|unsupported_capability",
        "stdio_bridge|error|E_BRIDGE_OUT_OF_SEQUENCE",
        "stdio_bridge|error|E_BRIDGE_ALREADY_INITIALIZED",
        "stdio_bridge|error|E_BRIDGE_UNSUPPORTED_PROTOCOL",
        "stdio_bridge|error|E_BRIDGE_MALFORMED_INITIALIZE",
        "stdio_bridge|error|E_BRIDGE_CLOSED",
        "stdio_bridge|error|E_BRIDGE_UNSUPPORTED_CAPABILITY",
        "stdio_bridge|degradation|streaming",
        "stdio_bridge|degradation|delivery_ack",
        "stdio_bridge|degradation|trace_context",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
