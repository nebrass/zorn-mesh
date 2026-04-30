use std::sync::mpsc;

use zornmesh_cli::broker::Broker;
use zornmesh_cli::core::{CoordinationOutcomeKind, CoordinationStage, Envelope, NackReasonCategory};

#[test]
fn publish_reports_transport_acceptance_separately_from_durability() {
    let broker = Broker::new();
    let (tx, rx) = mpsc::channel();
    let _subscription = broker
        .subscribe("mesh.trace.created", tx)
        .expect("subscription registers");
    let envelope = Envelope::new(
        "agent.local/publisher",
        "mesh.trace.created",
        b"{}".to_vec(),
    )
    .expect("valid envelope");

    let receipt = broker.publish(envelope).expect("publish is accepted");

    assert_eq!(receipt.delivery_attempts(), 1);
    assert_eq!(
        receipt.transport_outcome().kind(),
        CoordinationOutcomeKind::Accepted
    );
    assert_eq!(
        receipt.transport_outcome().stage(),
        CoordinationStage::Transport
    );
    assert_eq!(
        receipt.durable_outcome().kind(),
        CoordinationOutcomeKind::Failed
    );
    assert_eq!(
        receipt.durable_outcome().stage(),
        CoordinationStage::Durable
    );
    assert_eq!(
        receipt.durable_outcome().code(),
        "E_PERSISTENCE_UNAVAILABLE"
    );
    assert!(
        rx.try_recv().is_ok(),
        "transport acceptance still routes delivery"
    );
}

#[test]
fn ack_and_nack_record_delivery_outcomes_with_safe_reason_categories() {
    let broker = Broker::new();

    let ack = broker
        .record_ack("delivery-corr-1-1")
        .expect("ack outcome records");
    let nack = broker
        .record_nack("delivery-corr-1-2", NackReasonCategory::Processing)
        .expect("nack outcome records");
    let outcomes = broker.delivery_outcomes();

    assert_eq!(ack.kind(), CoordinationOutcomeKind::Acknowledged);
    assert_eq!(ack.stage(), CoordinationStage::Delivery);
    assert_eq!(ack.reason(), None);
    assert_eq!(nack.kind(), CoordinationOutcomeKind::Rejected);
    assert_eq!(nack.stage(), CoordinationStage::Delivery);
    assert_eq!(nack.reason(), Some(NackReasonCategory::Processing));
    assert_eq!(outcomes, vec![ack, nack]);
}
