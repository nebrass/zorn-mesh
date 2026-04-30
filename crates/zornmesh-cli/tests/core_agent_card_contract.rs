use zornmesh_cli::core::{
    AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardError, AgentCardErrorCode, AgentCardInput,
    AgentCardTransport,
};

fn input(stable_id: &str) -> AgentCardInput {
    AgentCardInput {
        profile_version: AGENT_CARD_PROFILE_VERSION.to_owned(),
        stable_id: stable_id.to_owned(),
        display_name: "  Test Agent  ".to_owned(),
        transport: "  unix  ".to_owned(),
        source: "agent.local/source".to_owned(),
    }
}

#[test]
fn canonical_normalization_trims_and_lowercases_transport() {
    let card = AgentCard::from_input(input("agent.local/Sample")).expect("valid card");
    assert_eq!(card.profile_version(), AGENT_CARD_PROFILE_VERSION);
    assert_eq!(card.stable_id(), "agent.local/Sample");
    assert_eq!(card.canonical_stable_id(), "agent.local/sample");
    assert_eq!(card.transport(), AgentCardTransport::Unix);
    assert_eq!(card.display_name(), "Test Agent");
    // raw input is retained for audit
    assert_eq!(card.raw_display_name(), "  Test Agent  ");
    assert_eq!(card.raw_transport(), "  unix  ");
}

#[test]
fn missing_required_fields_return_typed_validation_errors() {
    let mut bad = input("agent.local/sample");
    bad.stable_id = String::new();
    let err: AgentCardError = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::MissingStableId);

    let mut bad = input("agent.local/sample");
    bad.display_name = "   ".to_owned();
    let err = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::MissingDisplayName);

    let mut bad = input("agent.local/sample");
    bad.transport = String::new();
    let err = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::MissingTransport);

    let mut bad = input("agent.local/sample");
    bad.source = String::new();
    let err = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::MissingSource);
}

#[test]
fn unsupported_profile_version_is_rejected() {
    let mut bad = input("agent.local/sample");
    bad.profile_version = "agentcard.v999".to_owned();
    let err = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::UnsupportedVersion);
}

#[test]
fn unsupported_transport_is_rejected_with_typed_error() {
    let mut bad = input("agent.local/sample");
    bad.transport = "carrier-pigeon".to_owned();
    let err = AgentCard::from_input(bad).unwrap_err();
    assert_eq!(err.code(), AgentCardErrorCode::UnsupportedTransport);
}

#[test]
fn compatible_returns_same_canonical_id_for_case_variants() {
    let a = AgentCard::from_input(input("agent.local/Sample")).unwrap();
    let b = AgentCard::from_input(input("agent.local/SAMPLE")).unwrap();
    assert_eq!(a.canonical_stable_id(), b.canonical_stable_id());
    assert!(a.is_compatible_with(&b));
}

#[test]
fn incompatible_when_transport_or_version_differ() {
    let a = AgentCard::from_input(input("agent.local/sample")).unwrap();
    let mut other_input = input("agent.local/sample");
    other_input.transport = "tcp".to_owned();
    let b = AgentCard::from_input(other_input).unwrap();
    assert!(!a.is_compatible_with(&b));
}

#[test]
fn fixture_pins_agent_card_profile_version_and_required_fields() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "agent_card|version|agentcard.v1",
        "agent_card|required|stable_id",
        "agent_card|required|display_name",
        "agent_card|required|transport",
        "agent_card|required|source",
        "agent_card|transport|unix",
        "agent_card|transport|tcp",
        "agent_card|transport|in_process",
        "agent_card|error|E_AGENT_CARD_MISSING_STABLE_ID",
        "agent_card|error|E_AGENT_CARD_MISSING_DISPLAY_NAME",
        "agent_card|error|E_AGENT_CARD_MISSING_TRANSPORT",
        "agent_card|error|E_AGENT_CARD_MISSING_SOURCE",
        "agent_card|error|E_AGENT_CARD_UNSUPPORTED_VERSION",
        "agent_card|error|E_AGENT_CARD_UNSUPPORTED_TRANSPORT",
        "agent_card|error|E_AGENT_CARD_CONFLICT",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
