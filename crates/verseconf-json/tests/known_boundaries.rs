//! Characterization of frozen limitations, not recommended security policy.

use verseconf_json::{check_write_json, check_write_json_with, JsonGuard};

#[test]
fn known_boundary_non_tls_verify_email_is_falsely_refused() {
    let refusal = check_write_json(r#"{"verify_email": true}"#, r#"{"verify_email": false}"#)
        .expect_err("the frozen rule matches verify in non-TLS field names");
    assert_eq!(refusal.code(), "security_rejected");
    let details = refusal.details().to_string();
    assert!(details.contains("SEC-005"));
    assert!(details.contains("verify_email"));
}

#[test]
fn known_boundary_node_tls_environment_switch_is_not_detected() {
    check_write_json("{}", r#"{"NODE_TLS_REJECT_UNAUTHORIZED": "0"}"#)
        .expect("the frozen rule does not recognize this TLS environment switch");
}

#[test]
fn known_boundary_type_drift_is_allowed_without_a_schema() {
    check_write_json(r#"{"port": 8080}"#, r#"{"port": "8080"}"#)
        .expect("without a schema the gate does not enforce the original type");
}

#[test]
fn explicit_schema_rejects_the_same_type_drift() {
    let schema = "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n";
    let refusal = check_write_json_with(
        r#"{"port": 8080}"#,
        r#"{"port": "8080"}"#,
        &JsonGuard::with_schema(schema),
    )
    .expect_err("an explicit schema provides the missing type constraint");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn known_boundary_empty_object_is_allowed_without_a_schema() {
    check_write_json(r#"{"port": 8080, "workers": 2}"#, "{}")
        .expect("the gate cannot infer which application settings are required");
}

#[test]
fn known_boundary_changed_credential_at_same_location_is_not_a_new_instance() {
    check_write_json(
        r#"{"db_password": "hunter2"}"#,
        r#"{"db_password": "hunter3"}"#,
    )
    .expect("the frozen difference counts rule and location, not changed secret values");
}
