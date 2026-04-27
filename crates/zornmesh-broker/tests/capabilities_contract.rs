use zornmesh_broker::{
    AgentRegistrationOutcome, Broker, CapabilityChangeKind, CapabilityDeclarationOutcome,
    CapabilityErrorCode,
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
        .with_summary(format!("capability {id}@{version}"))
        .with_schema_ref(CapabilitySchemaDialect::TypeBox, format!("{id}.{version}.schema"))
        .build()
        .expect("valid capability descriptor")
}

fn register_agent(broker: &Broker, stable_id: &str) -> AgentCard {
    let card = AgentCard::from_input(agent_input(stable_id)).unwrap();
    let outcome = broker.register_agent_card(card.clone()).unwrap();
    match outcome {
        AgentRegistrationOutcome::Registered { canonical }
        | AgentRegistrationOutcome::Compatible { canonical } => canonical,
        AgentRegistrationOutcome::Conflict { .. } => panic!("conflict during test setup"),
    }
}

#[test]
fn declaring_offered_and_consumed_capabilities_associates_with_agent() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/alpha");

    let outcome = broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![
                cap("compute.run", "v1", CapabilityDirection::Offered),
                cap("storage.write", "v2", CapabilityDirection::Consumed),
            ],
        )
        .expect("declaration accepted");
    let CapabilityDeclarationOutcome::Updated {
        offered,
        consumed,
        change_kind,
    } = outcome;
    assert_eq!(offered.len(), 1);
    assert_eq!(consumed.len(), 1);
    assert_eq!(offered[0].capability_id(), "compute.run");
    assert_eq!(consumed[0].capability_id(), "storage.write");
    assert_eq!(change_kind, CapabilityChangeKind::Initial);

    let summary = broker
        .inspect_agent_capabilities(agent.canonical_stable_id())
        .expect("agent has capabilities");
    assert_eq!(summary.offered.len(), 1);
    assert_eq!(summary.consumed.len(), 1);
}

#[test]
fn invalid_capability_id_or_version_is_rejected_without_partial_mutation() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/alpha");

    let invalid_id = CapabilityDescriptor::builder(
        "bad id with spaces",
        "v1",
        CapabilityDirection::Offered,
    )
    .with_summary("x")
    .with_schema_ref(CapabilitySchemaDialect::TypeBox, "bad.schema")
    .build();
    assert!(invalid_id.is_err());

    let invalid_version = CapabilityDescriptor::builder(
        "compute.run",
        "",
        CapabilityDirection::Offered,
    )
    .with_summary("x")
    .with_schema_ref(CapabilitySchemaDialect::TypeBox, "bad.schema")
    .build();
    assert!(invalid_version.is_err());

    // Mixed list: one valid + one invalid → entire declaration rejected.
    let outcome = broker.declare_capabilities(
        agent.canonical_stable_id(),
        vec![cap("compute.run", "v1", CapabilityDirection::Offered)],
    );
    assert!(outcome.is_ok());

    // After above accepted decl, an invalid update must not wipe state.
    let bad_descriptor = cap("compute.run", "v1", CapabilityDirection::Offered);
    let bad_descriptor = CapabilityDescriptor::builder(
        bad_descriptor.capability_id(),
        bad_descriptor.version(),
        bad_descriptor.direction(),
    )
    .with_summary(bad_descriptor.summary())
    .with_schema_ref(CapabilitySchemaDialect::TypeBox, "")
    .build();
    assert!(bad_descriptor.is_err());

    // State unchanged.
    let summary = broker
        .inspect_agent_capabilities(agent.canonical_stable_id())
        .expect("capabilities still present");
    assert_eq!(summary.offered.len(), 1);
    assert_eq!(summary.consumed.len(), 0);
}

#[test]
fn declaring_capabilities_for_unknown_agent_returns_typed_error() {
    let broker = Broker::new();
    let err = broker
        .declare_capabilities(
            "agent.local/missing",
            vec![cap("compute.run", "v1", CapabilityDirection::Offered)],
        )
        .unwrap_err();
    assert_eq!(err.code(), CapabilityErrorCode::AgentNotFound);
}

#[test]
fn second_declaration_emits_changed_kind_and_event() {
    let broker = Broker::new();
    let agent = register_agent(&broker, "agent.local/alpha");

    broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![cap("compute.run", "v1", CapabilityDirection::Offered)],
        )
        .unwrap();
    let second = broker
        .declare_capabilities(
            agent.canonical_stable_id(),
            vec![
                cap("compute.run", "v1", CapabilityDirection::Offered),
                cap("compute.run", "v2", CapabilityDirection::Offered),
            ],
        )
        .unwrap();
    let CapabilityDeclarationOutcome::Updated { change_kind, .. } = second;
    assert_eq!(change_kind, CapabilityChangeKind::Changed);

    let events = broker.capability_change_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind(), CapabilityChangeKind::Initial);
    assert_eq!(events[1].kind(), CapabilityChangeKind::Changed);
    assert_eq!(events[1].agent_canonical_id(), agent.canonical_stable_id());
}

#[test]
fn list_agents_with_capabilities_returns_all_registered_agents() {
    let broker = Broker::new();
    let alpha = register_agent(&broker, "agent.local/alpha");
    let beta = register_agent(&broker, "agent.local/beta");
    broker
        .declare_capabilities(
            alpha.canonical_stable_id(),
            vec![cap("compute.run", "v1", CapabilityDirection::Offered)],
        )
        .unwrap();
    broker
        .declare_capabilities(
            beta.canonical_stable_id(),
            vec![cap("storage.write", "v1", CapabilityDirection::Consumed)],
        )
        .unwrap();

    let mut listed = broker.list_agents_with_capabilities();
    listed.sort_by(|a, b| a.agent.canonical_stable_id().cmp(b.agent.canonical_stable_id()));
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].agent.canonical_stable_id(), "agent.local/alpha");
    assert_eq!(listed[0].offered.len(), 1);
    assert_eq!(listed[1].agent.canonical_stable_id(), "agent.local/beta");
    assert_eq!(listed[1].consumed.len(), 1);
}

#[test]
fn fixture_pins_capability_taxonomy() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "capability|direction|offered",
        "capability|direction|consumed",
        "capability|direction|both",
        "capability|schema_dialect|typebox",
        "capability|schema_dialect|json_schema",
        "capability|change|initial",
        "capability|change|changed",
        "capability|error|E_CAPABILITY_INVALID_ID",
        "capability|error|E_CAPABILITY_INVALID_VERSION",
        "capability|error|E_CAPABILITY_INVALID_SCHEMA",
        "capability|error|E_CAPABILITY_AGENT_NOT_FOUND",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
