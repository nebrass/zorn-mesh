use zornmesh_cli::core::{
    REDACTION_MARKER, RedactionError, RedactionErrorCode, RedactionPolicy, Redactor, SecretValue,
};

#[test]
fn secret_value_debug_and_display_emit_redaction_marker_only() {
    let secret = SecretValue::new("hunter2-the-real-password");
    let displayed = format!("{secret}");
    let debugged = format!("{secret:?}");
    assert_eq!(displayed, REDACTION_MARKER);
    assert!(debugged.contains(REDACTION_MARKER));
    assert!(!displayed.contains("hunter2"));
    assert!(!debugged.contains("hunter2"));
}

#[test]
fn secret_value_only_reveals_through_explicit_expose_method() {
    let secret = SecretValue::new("hunter2");
    // Reading the secret value requires explicit, named-by-design call.
    assert_eq!(secret.expose_secret(), "hunter2");
}

#[test]
fn redactor_redacts_named_fields_in_structured_record() {
    let policy = RedactionPolicy::new(["api_key", "password"]).expect("valid policy");
    let redactor = Redactor::new(policy);
    let record = vec![
        ("user".to_owned(), "alice".to_owned()),
        ("api_key".to_owned(), "sk-live-abc123".to_owned()),
        ("password".to_owned(), "hunter2".to_owned()),
        ("subject".to_owned(), "agent.work.compute".to_owned()),
    ];

    let redacted = redactor.redact_pairs(&record);

    let assoc: std::collections::HashMap<_, _> = redacted.into_iter().collect();
    assert_eq!(assoc.get("user").unwrap(), "alice");
    assert_eq!(assoc.get("subject").unwrap(), "agent.work.compute");
    assert_eq!(assoc.get("api_key").unwrap(), REDACTION_MARKER);
    assert_eq!(assoc.get("password").unwrap(), REDACTION_MARKER);
}

#[test]
fn redactor_redacts_substring_matches_in_free_text_logs() {
    let policy = RedactionPolicy::new(["password"]).unwrap();
    let redactor = Redactor::new(policy);
    let log_line = "user=alice password=hunter2 retrying after error";
    let scrubbed = redactor.redact_text(log_line, &[("password", "hunter2")]);
    assert!(!scrubbed.contains("hunter2"));
    assert!(scrubbed.contains(REDACTION_MARKER));
    assert!(scrubbed.contains("user=alice"));
}

#[test]
fn empty_or_invalid_redaction_policy_returns_typed_error() {
    let empty: Result<RedactionPolicy, RedactionError> = RedactionPolicy::new::<&str, _>([]);
    assert!(empty.is_err());
    assert_eq!(
        empty.unwrap_err().code(),
        RedactionErrorCode::EmptyFieldList
    );

    let blank = RedactionPolicy::new(["", "valid_field"]);
    assert!(blank.is_err());
    assert_eq!(blank.unwrap_err().code(), RedactionErrorCode::InvalidField);
}

#[test]
fn ambiguous_secret_marker_defaults_to_redacted() {
    let policy = RedactionPolicy::new(["?secret_or_not?"]).unwrap();
    let redactor = Redactor::new(policy);
    let pairs = vec![
        ("user".to_owned(), "alice".to_owned()),
        ("?secret_or_not?".to_owned(), "ambiguous-value".to_owned()),
    ];
    let scrubbed = redactor.redact_pairs(&pairs);
    let assoc: std::collections::HashMap<_, _> = scrubbed.into_iter().collect();
    assert_eq!(assoc.get("?secret_or_not?").unwrap(), REDACTION_MARKER);
}

#[test]
fn fixture_pins_redaction_contract() {
    let fixture = include_str!("../../../fixtures/coordination/contract.txt");
    for row in [
        "redaction|marker|[REDACTED]",
        "redaction|annotation|secret",
        "redaction|annotation|sensitive",
        "redaction|default|secret_on_ambiguity",
        "redaction|error|E_REDACTION_EMPTY_FIELD_LIST",
        "redaction|error|E_REDACTION_INVALID_FIELD",
    ] {
        assert!(fixture.contains(row), "missing fixture row {row}");
    }
}
