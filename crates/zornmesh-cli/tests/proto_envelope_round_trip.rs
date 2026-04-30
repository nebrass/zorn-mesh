use zornmesh_cli::core::Envelope;
use zornmesh_cli::proto::{decode_envelope, encode_envelope};

#[test]
fn envelope_round_trips_through_proto_boundary() {
    let envelope = Envelope::with_trace_context(
        "agent.local/dev",
        "mesh.trace.created",
        b"{\"trace_id\":\"trace-1\"}".to_vec(),
        42,
        "corr-proto-trace",
        "application/json",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        Some("rojo=00f067aa0ba902b7"),
    )
    .expect("valid smoke envelope");

    let encoded = encode_envelope(&envelope);
    let decoded = decode_envelope(&encoded).expect("encoded envelope decodes");

    assert_eq!(decoded, envelope);
    assert_eq!(
        decoded.trace_context().traceparent(),
        envelope.trace_context().traceparent()
    );
    assert_eq!(
        decoded.trace_context().tracestate(),
        envelope.trace_context().tracestate()
    );
}
