use zornmesh_broker::{AgentRegistrationOutcome, Broker, CapabilityDeclarationOutcome};
use zornmesh_core::{
    AGENT_CARD_PROFILE_VERSION, AgentCard, AgentCardInput, CapabilityDescriptor,
    CapabilityDirection, CapabilitySchemaDialect, REDACTION_MARKER,
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

fn register_agent(broker: &Broker, stable_id: &str) -> AgentCard {
    let card = AgentCard::from_input(agent_input(stable_id)).unwrap();
    let outcome = broker.register_agent_card(card.clone()).unwrap();
    match outcome {
        AgentRegistrationOutcome::Registered { canonical }
        | AgentRegistrationOutcome::Compatible { canonical } => canonical,
        AgentRegistrationOutcome::Conflict { .. } => panic!("conflict"),
    }
}

fn cap_with_secret_field(secret_field: &str) -> CapabilityDescriptor {
    CapabilityDescriptor::builder("auth.login", "v1", CapabilityDirection::Offered)
        .with_summary("Login with credentials")
        .with_schema_ref(CapabilitySchemaDialect::TypeBox, "auth.login.v1")
        .with_secret_field(secret_field)
        .build()
        .expect("valid capability descriptor")
}

#[test]
fn capability_descriptor_secret_fields_round_trip_through_declaration() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/auth");
    let cap = cap_with_secret_field("password");

    let outcome = broker
        .declare_capabilities(agent.canonical_stable_id(), vec![cap.clone()])
        .unwrap();
    let CapabilityDeclarationOutcome::Updated { offered, .. } = outcome;
    assert_eq!(offered.len(), 1);
    assert!(offered[0].secret_fields().iter().any(|f| f == "password"));
}

#[test]
fn safe_summary_redacts_secret_fields_in_capability_descriptor() {
    let cap = cap_with_secret_field("password");
    let summary = cap.safe_summary_pairs(&[("password", "hunter2"), ("user", "alice")]);

    let assoc: std::collections::HashMap<_, _> = summary.into_iter().collect();
    assert_eq!(assoc.get("password").unwrap(), REDACTION_MARKER);
    assert_eq!(assoc.get("user").unwrap(), "alice");
}
