use zornmesh_core::Envelope;
use zornmesh_proto::{decode_envelope, encode_envelope};

#[test]
fn envelope_round_trips_through_proto_boundary() {
    let envelope = Envelope::new(
        "agent.local/dev",
        "mesh.trace.created",
        b"{\"trace_id\":\"trace-1\"}".to_vec(),
    )
    .expect("valid smoke envelope");

    let encoded = encode_envelope(&envelope);
    let decoded = decode_envelope(&encoded).expect("encoded envelope decodes");

    assert_eq!(decoded, envelope);
}
