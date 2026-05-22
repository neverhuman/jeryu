use jsonschema::validator_for;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn schema_value() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/vault-doctor.schema.json");
    let schema = fs::read_to_string(&path).expect("read vault doctor schema");
    serde_json::from_str(&schema).expect("parse vault doctor schema")
}

fn sample_report_value() -> serde_json::Value {
    json!({
        "status": {
            "addr": "http://127.0.0.1:18200",
            "reachable": true,
            "initialized": true,
            "sealed": false,
            "healthy": true,
            "token_present": true,
            "mount": "secret",
            "prefix": "veox",
            "bootstrap_file": "/tmp/vault/bootstrap.json",
            "env_file": "/tmp/vault/vault.env"
        },
        "issues": [],
        "ok": true
    })
}

#[test]
fn vault_doctor_report_matches_schema() {
    let schema = schema_value();
    let validator = validator_for(&schema).expect("compile vault doctor schema");
    let value = sample_report_value();
    assert!(
        validator.is_valid(&value),
        "report did not match schema: {value}"
    );
}
