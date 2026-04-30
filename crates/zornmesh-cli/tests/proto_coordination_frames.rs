use std::io::Cursor;

use zornmesh_cli::core::{
    CoordinationOutcome, CoordinationOutcomeKind, CoordinationStage, Envelope, NackReasonCategory,
};
use zornmesh_cli::proto::{
    ClientFrame, DeliveryOutcomeFrame, FrameStatus, MAX_FRAME_BYTES, ProtoError, SendResultFrame,
    ServerFrame, read_client_frame, read_server_frame, write_client_frame, write_server_frame,
};

#[test]
fn send_result_frame_carries_transport_and_durable_outcomes() {
    let transport = CoordinationOutcome::accepted("accepted for routing", 2);
    let durable = CoordinationOutcome::persistence_unavailable();
    let frame = ServerFrame::SendResult(SendResultFrame::new(
        FrameStatus::Accepted,
        "ACCEPTED",
        "accepted for routing; delivery_attempts=2",
        transport.clone(),
        Some(durable.clone()),
    ));
    let mut bytes = Vec::new();

    write_server_frame(&mut bytes, &frame).expect("send result frame encodes");
    let decoded = read_server_frame(&mut Cursor::new(bytes)).expect("send result frame decodes");

    match decoded {
        ServerFrame::SendResult(result) => {
            assert_eq!(result.status(), FrameStatus::Accepted);
            assert_eq!(result.outcome(), &transport);
            assert_eq!(result.durable_outcome(), Some(&durable));
        }
        ServerFrame::Delivery { .. } | ServerFrame::DeliveryOutcome(_) => {
            panic!("decoded unexpected server frame")
        }
    }
}

#[test]
fn ack_and_nack_client_frames_round_trip() {
    for frame in [
        ClientFrame::Ack {
            delivery_id: "delivery-corr-1-1".to_owned(),
        },
        ClientFrame::Nack {
            delivery_id: "delivery-corr-1-2".to_owned(),
            reason: NackReasonCategory::Processing,
        },
    ] {
        let mut bytes = Vec::new();

        write_client_frame(&mut bytes, &frame).expect("client frame encodes");
        let decoded = read_client_frame(&mut Cursor::new(bytes)).expect("client frame decodes");

        assert_eq!(decoded, frame);
    }
}

#[test]
fn delivery_and_delivery_outcome_frames_round_trip() {
    let envelope = Envelope::new("agent.local/source", "mesh.trace.created", b"{}".to_vec())
        .expect("valid envelope");
    let delivery = ServerFrame::Delivery {
        delivery_id: "delivery-corr-1-1".to_owned(),
        envelope: envelope.clone(),
        attempt: 1,
    };
    let outcome = ServerFrame::DeliveryOutcome(DeliveryOutcomeFrame::new(
        "delivery-corr-1-1",
        CoordinationOutcome::new(
            CoordinationOutcomeKind::Acknowledged,
            CoordinationStage::Delivery,
            "ACKNOWLEDGED",
            "delivery acknowledged",
            false,
            true,
            1,
        ),
        None,
    ));

    for frame in [delivery, outcome] {
        let mut bytes = Vec::new();
        write_server_frame(&mut bytes, &frame).expect("server frame encodes");

        let decoded = read_server_frame(&mut Cursor::new(bytes)).expect("server frame decodes");

        assert_eq!(decoded, frame);
    }
}

#[test]
fn malformed_frames_fail_with_stable_protocol_errors_before_decoding_state() {
    let oversize_len = u32::try_from(MAX_FRAME_BYTES + 1)
        .expect("frame limit fits in u32")
        .to_be_bytes();
    let unsupported_version = framed_body(&[b'Z', b'M', 0, 99, 1]);
    let unknown_type = framed_body(&[b'Z', b'M', 0, 1, 255]);
    let truncated_body = [0, 0, 0, 10, b'Z', b'M', 0, 1, 1];

    assert!(matches!(
        read_client_frame(&mut Cursor::new(oversize_len)),
        Err(ProtoError::FrameTooLarge { .. })
    ));
    assert!(matches!(
        read_client_frame(&mut Cursor::new(unsupported_version)),
        Err(ProtoError::UnsupportedVersion(99))
    ));
    assert!(matches!(
        read_client_frame(&mut Cursor::new(unknown_type)),
        Err(ProtoError::UnknownFrameType(255))
    ));
    assert!(matches!(
        read_client_frame(&mut Cursor::new(truncated_body)),
        Err(ProtoError::Truncated("frame_body"))
    ));
}

fn framed_body(body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &u32::try_from(body.len())
            .expect("test body fits in u32")
            .to_be_bytes(),
    );
    bytes.extend_from_slice(body);
    bytes
}
