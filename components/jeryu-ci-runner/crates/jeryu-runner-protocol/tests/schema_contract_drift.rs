use jeryu_runner_protocol::PROTOCOL_VERSION;
use jeryu_runner_protocol::wire::{MAX_CAPACITY, MAX_LABELS, MAX_RESULT_ITEMS, MAX_STEPS};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA_TEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/jeryu.runner.v1.schema.json"
));
const MOD_SOURCE: &str = include_str!("../src/wire/mod.rs");
const BODY_SOURCE: &str = include_str!("../src/wire/body.rs");
const CONTEXT_SOURCE: &str = include_str!("../src/wire/context.rs");
const HEARTBEAT_SOURCE: &str = include_str!("../src/wire/heartbeat.rs");
const JOB_SOURCE: &str = include_str!("../src/wire/job.rs");
const LEASE_SOURCE: &str = include_str!("../src/wire/lease.rs");
const REGISTER_SOURCE: &str = include_str!("../src/wire/register.rs");
const RESULT_SOURCE: &str = include_str!("../src/wire/result.rs");

const OBJECTS: [(&str, &str); 16] = [
    ("RunnerContext", CONTEXT_SOURCE),
    ("ExecutionContext", CONTEXT_SOURCE),
    ("WireStep", BODY_SOURCE),
    ("WireCacheMount", BODY_SOURCE),
    ("WireArtifactPath", BODY_SOURCE),
    ("WireJobRequest", JOB_SOURCE),
    ("WireJobResult", JOB_SOURCE),
    ("RegisterRequest", REGISTER_SOURCE),
    ("RegisterAck", REGISTER_SOURCE),
    ("HeartbeatRequest", HEARTBEAT_SOURCE),
    ("HeartbeatAck", HEARTBEAT_SOURCE),
    ("LeaseRequest", LEASE_SOURCE),
    ("LeaseGrant", LEASE_SOURCE),
    ("LeaseAck", LEASE_SOURCE),
    ("ResultRequest", RESULT_SOURCE),
    ("ResultAck", RESULT_SOURCE),
];

const ENUMS: [(&str, &str); 5] = [
    ("WireCacheMode", BODY_SOURCE),
    ("WireArtifactWhen", BODY_SOURCE),
    ("WireJobOutcome", JOB_SOURCE),
    ("LeaseDecision", LEASE_SOURCE),
    ("ResultDecision", RESULT_SOURCE),
];

const MESSAGES: [(&str, &str); 8] = [
    ("RegisterRequest", "REGISTER_REQUEST"),
    ("RegisterAck", "REGISTER_ACK"),
    ("HeartbeatRequest", "HEARTBEAT_REQUEST"),
    ("HeartbeatAck", "HEARTBEAT_ACK"),
    ("LeaseRequest", "LEASE_REQUEST"),
    ("LeaseAck", "LEASE_ACK"),
    ("ResultRequest", "RESULT_REQUEST"),
    ("ResultAck", "RESULT_ACK"),
];

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))
}

fn string_set(value: &Value, label: &str) -> Result<BTreeSet<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{label} must be an array"))?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("{label} contains a non-string"))
        })
        .collect()
}

fn pub_struct_fields(source: &str, name: &str) -> Result<BTreeMap<String, String>, String> {
    let marker = format!("pub struct {name} {{");
    let mut inside = false;
    let mut fields = BTreeMap::new();
    for line in source.lines() {
        let line = line.trim();
        if line == marker {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line == "}" {
            return (!fields.is_empty())
                .then_some(fields)
                .ok_or_else(|| format!("Rust struct {name} has no public fields"));
        }
        if let Some(field) = line.strip_prefix("pub ") {
            let (field, rust_type) = field
                .split_once(':')
                .ok_or_else(|| format!("unparseable Rust field in {name}: {line}"))?;
            let rust_type = rust_type.trim().trim_end_matches(',');
            if field.is_empty() || rust_type.is_empty() {
                return Err(format!("empty Rust field or type in {name}: {line}"));
            }
            if fields
                .insert(field.to_string(), rust_type.to_string())
                .is_some()
            {
                return Err(format!("duplicate Rust field in {name}: {field}"));
            }
        }
    }
    Err(format!("Rust struct {name} not found"))
}

fn schema_kinds(
    schema: &Value,
    definitions: &Map<String, Value>,
    depth: usize,
) -> Result<BTreeSet<String>, String> {
    if depth > 16 {
        return Err("schema reference nesting exceeds 16 levels".to_string());
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference
            .strip_prefix("#/$defs/")
            .ok_or_else(|| format!("non-local schema reference: {reference}"))?;
        let target = definitions
            .get(name)
            .ok_or_else(|| format!("missing schema reference target: {name}"))?;
        return schema_kinds(target, definitions, depth + 1);
    }

    let mut kinds = BTreeSet::new();
    if let Some(schema_type) = schema.get("type") {
        match schema_type {
            Value::String(value) => {
                kinds.insert(value.clone());
            }
            Value::Array(values) => {
                for value in values {
                    kinds.insert(
                        value
                            .as_str()
                            .ok_or_else(|| "schema type array contains a non-string".to_string())?
                            .to_string(),
                    );
                }
            }
            _ => return Err("schema type must be a string or string array".to_string()),
        }
    }
    if let Some(constant) = schema.get("const") {
        kinds.insert(json_kind(constant)?.to_string());
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        for value in values {
            kinds.insert(json_kind(value)?.to_string());
        }
    }
    for combinator in ["allOf", "anyOf", "oneOf"] {
        if let Some(values) = schema.get(combinator).and_then(Value::as_array) {
            for value in values {
                kinds.extend(schema_kinds(value, definitions, depth + 1)?);
            }
        }
    }
    Ok(kinds)
}

fn json_kind(value: &Value) -> Result<&'static str, String> {
    match value {
        Value::Null => Ok("null"),
        Value::Bool(_) => Ok("boolean"),
        Value::Number(_) => Ok("integer"),
        Value::String(_) => Ok("string"),
        Value::Array(_) => Ok("array"),
        Value::Object(_) => Ok("object"),
    }
}

fn contains_ref(schema: &Value, expected: &str) -> bool {
    if schema.get("$ref").and_then(Value::as_str) == Some(expected) {
        return true;
    }
    ["allOf", "anyOf", "oneOf"].into_iter().any(|combinator| {
        schema
            .get(combinator)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| contains_ref(value, expected)))
    })
}

fn generic_inner<'a>(rust_type: &'a str, wrapper: &str) -> Option<&'a str> {
    rust_type
        .strip_prefix(wrapper)
        .and_then(|value| value.strip_suffix('>'))
}

fn expected_scalar_kind(rust_type: &str) -> Option<&'static str> {
    match rust_type {
        "String" | "WireRunnerClass" | "WireCacheMode" | "WireArtifactWhen" | "WireJobOutcome"
        | "LeaseDecision" | "ResultDecision" => Some("string"),
        "u32" | "u64" | "i32" => Some("integer"),
        "bool" => Some("boolean"),
        "RunnerContext" | "ExecutionContext" | "WireStep" | "WireCacheMount"
        | "WireArtifactPath" | "WireJobRequest" | "WireJobResult" | "RegisterRequest"
        | "RegisterAck" | "HeartbeatRequest" | "HeartbeatAck" | "LeaseRequest" | "LeaseGrant"
        | "LeaseAck" | "ResultRequest" | "ResultAck" => Some("object"),
        _ => None,
    }
}

fn schema_matches_rust_type(
    schema: &Value,
    rust_type: &str,
    definitions: &Map<String, Value>,
) -> Result<bool, String> {
    if let Some(inner) = generic_inner(rust_type, "Option<") {
        let mut expected = BTreeSet::from(["null".to_string()]);
        expected.insert(
            expected_scalar_kind(inner)
                .ok_or_else(|| format!("unsupported optional Rust wire type: {rust_type}"))?
                .to_string(),
        );
        let kinds = schema_kinds(schema, definitions, 0)?;
        let reference_matches = match inner {
            "String" | "i32" | "u32" | "u64" | "bool" => true,
            "WireRunnerClass" => contains_ref(schema, "#/$defs/RunnerClass"),
            name => contains_ref(schema, &format!("#/$defs/{name}")),
        };
        return Ok(kinds == expected && reference_matches);
    }
    if let Some(inner) = generic_inner(rust_type, "Vec<") {
        if schema_kinds(schema, definitions, 0)? != BTreeSet::from(["array".to_string()]) {
            return Ok(false);
        }
        let items = schema
            .get("items")
            .ok_or_else(|| format!("array schema for {rust_type} has no items"))?;
        return schema_matches_rust_type(items, inner, definitions);
    }
    if rust_type == "BTreeMap<String, String>" {
        if schema_kinds(schema, definitions, 0)? != BTreeSet::from(["object".to_string()]) {
            return Ok(false);
        }
        let map_schema = if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            definitions
                .get(reference.trim_start_matches("#/$defs/"))
                .ok_or_else(|| format!("missing map schema target: {reference}"))?
        } else if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
            let reference = all_of
                .iter()
                .find_map(|value| value.get("$ref").and_then(Value::as_str))
                .ok_or_else(|| "map allOf has no reference".to_string())?;
            definitions
                .get(reference.trim_start_matches("#/$defs/"))
                .ok_or_else(|| format!("missing map schema target: {reference}"))?
        } else {
            schema
        };
        let values = map_schema
            .get("additionalProperties")
            .ok_or_else(|| "map schema has no additionalProperties type".to_string())?;
        return Ok(schema_kinds(values, definitions, 0)? == BTreeSet::from(["string".to_string()]));
    }

    let expected_kind = expected_scalar_kind(rust_type)
        .ok_or_else(|| format!("unsupported Rust wire type: {rust_type}"))?;
    let reference_matches = match rust_type {
        "String" | "u32" | "u64" | "i32" | "bool" => true,
        "WireRunnerClass" => contains_ref(schema, "#/$defs/RunnerClass"),
        name => contains_ref(schema, &format!("#/$defs/{name}")),
    };
    Ok(
        schema_kinds(schema, definitions, 0)? == BTreeSet::from([expected_kind.to_string()])
            && reference_matches,
    )
}

fn pub_unit_enum_values(source: &str, name: &str) -> Result<Vec<String>, String> {
    let marker = format!("pub enum {name} {{");
    let mut inside = false;
    let mut variants = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line == marker {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line == "}" {
            return (!variants.is_empty())
                .then_some(variants)
                .ok_or_else(|| format!("Rust enum {name} has no variants"));
        }
        let variant = line.trim_end_matches(',');
        if variant.is_empty() || variant.starts_with("//") {
            continue;
        }
        if !variant.chars().all(char::is_alphanumeric) {
            return Err(format!("{name} is no longer a unit enum: {line}"));
        }
        variants.push(kebab_case(variant));
    }
    Err(format!("Rust enum {name} not found"))
}

fn kebab_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index != 0 {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

fn rust_string_const(source: &str, name: &str) -> Result<String, String> {
    let needle = format!("const {name}:");
    let line = source
        .lines()
        .find(|line| line.contains(&needle))
        .ok_or_else(|| format!("Rust constant {name} not found"))?;
    let value = line
        .split_once('=')
        .ok_or_else(|| format!("Rust constant {name} has no value"))?
        .1
        .trim()
        .trim_end_matches(';');
    serde_json::from_str(value).map_err(|error| format!("invalid string constant {name}: {error}"))
}

fn schema_u64(schema: &Value, pointer: &str) -> Result<u64, String> {
    schema
        .pointer(pointer)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing integer schema bound at {pointer}"))
}

fn validate_contract(schema: &Value) -> Result<(), String> {
    if schema.get("$schema").and_then(Value::as_str)
        != Some("https://json-schema.org/draft/2020-12/schema")
    {
        return Err("schema draft identity drifted".to_string());
    }

    let expected_refs: Vec<String> = MESSAGES
        .iter()
        .map(|(name, _)| format!("#/$defs/{name}"))
        .collect();
    let actual_refs: Vec<String> = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .ok_or_else(|| "top-level oneOf is missing".to_string())?
        .iter()
        .map(|entry| {
            entry
                .get("$ref")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| "top-level oneOf entry is not a ref".to_string())
        })
        .collect::<Result<_, _>>()?;
    if actual_refs != expected_refs {
        return Err(format!(
            "top-level messages differ: expected {expected_refs:?}, got {actual_refs:?}"
        ));
    }

    let definitions = object(
        schema
            .get("$defs")
            .ok_or_else(|| "$defs is missing".to_string())?,
        "$defs",
    )?;
    for (name, source) in OBJECTS {
        let rust_fields = pub_struct_fields(source, name)?;
        let rust_field_names: BTreeSet<String> = rust_fields.keys().cloned().collect();
        let definition = object(
            definitions
                .get(name)
                .ok_or_else(|| format!("schema definition {name} is missing"))?,
            name,
        )?;
        if definition.get("type").and_then(Value::as_str) != Some("object")
            || definition
                .get("additionalProperties")
                .and_then(Value::as_bool)
                != Some(false)
        {
            return Err(format!(
                "{name} must remain a closed object with additionalProperties=false"
            ));
        }
        let required = string_set(
            definition
                .get("required")
                .ok_or_else(|| format!("{name}.required is missing"))?,
            &format!("{name}.required"),
        )?;
        let properties: BTreeSet<String> = object(
            definition
                .get("properties")
                .ok_or_else(|| format!("{name}.properties is missing"))?,
            &format!("{name}.properties"),
        )?
        .keys()
        .cloned()
        .collect();
        if rust_field_names != required || rust_field_names != properties {
            return Err(format!(
                "{name} field drift: rust={rust_field_names:?} required={required:?} properties={properties:?}"
            ));
        }
        let schema_properties = object(
            definition
                .get("properties")
                .ok_or_else(|| format!("{name}.properties is missing"))?,
            &format!("{name}.properties"),
        )?;
        for (field, rust_type) in &rust_fields {
            let field_schema = schema_properties
                .get(field)
                .ok_or_else(|| format!("{name}.{field} schema is missing"))?;
            if !schema_matches_rust_type(field_schema, rust_type, definitions)? {
                return Err(format!(
                    "{name}.{field} type drift: Rust={rust_type}, schema={field_schema}"
                ));
            }
        }
        if properties.iter().any(|field| {
            let upper = field.to_ascii_uppercase();
            ["AUTHORIZATION", "CREDENTIAL", "PASSWORD", "SECRET", "TOKEN"]
                .iter()
                .any(|reserved| upper.contains(reserved))
        }) {
            return Err(format!("{name} exposes an authentication field"));
        }
    }

    for (name, source) in ENUMS {
        let rust_values = pub_unit_enum_values(source, name)?;
        let schema_values: Vec<String> = definitions
            .get(name)
            .and_then(|definition| definition.get("enum"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("schema enum {name} is missing"))?
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| format!("schema enum {name} contains a non-string"))
            })
            .collect::<Result<_, _>>()?;
        if rust_values != schema_values {
            return Err(format!(
                "{name} enum drift: rust={rust_values:?} schema={schema_values:?}"
            ));
        }
    }

    for (name, constant) in MESSAGES {
        let definition = object(
            definitions
                .get(name)
                .ok_or_else(|| format!("message {name} is missing"))?,
            name,
        )?;
        let properties = object(
            definition
                .get("properties")
                .ok_or_else(|| format!("{name}.properties is missing"))?,
            &format!("{name}.properties"),
        )?;
        let protocol = properties
            .get("protocol_version")
            .and_then(|value| value.get("const"))
            .and_then(Value::as_str);
        let message_type = properties
            .get("message_type")
            .and_then(|value| value.get("const"))
            .and_then(Value::as_str);
        let rust_message_type = rust_string_const(MOD_SOURCE, constant)?;
        if protocol != Some(PROTOCOL_VERSION) || message_type != Some(&rust_message_type) {
            return Err(format!(
                "{name} discriminator drift: protocol={protocol:?} message_type={message_type:?}"
            ));
        }
    }

    let runner_class = object(
        definitions
            .get("RunnerClass")
            .ok_or_else(|| "RunnerClass definition is missing".to_string())?,
        "RunnerClass",
    )?;
    let variants = runner_class
        .get("oneOf")
        .and_then(Value::as_array)
        .ok_or_else(|| "RunnerClass.oneOf is missing".to_string())?;
    let builtins: BTreeSet<String> = string_set(
        variants
            .first()
            .and_then(|entry| entry.get("enum"))
            .ok_or_else(|| "RunnerClass built-ins are missing".to_string())?,
        "RunnerClass built-ins",
    )?;
    let expected_builtins: BTreeSet<String> = [
        "native-rust-hot",
        "native-rust-clean",
        "crategraph-delta",
        "nextest-capsule",
        "agent-guard",
        "merge-spec",
        "release-hermetic",
        "microvm-rust",
        "oci-docker",
        "k8s-oci",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect();
    if builtins != expected_builtins
        || expected_builtins
            .iter()
            .any(|value| !CONTEXT_SOURCE.contains(&format!("\"{value}\"")))
        || variants
            .get(1)
            .and_then(|entry| entry.get("pattern"))
            .and_then(Value::as_str)
            != Some("^custom:[a-z0-9](?:[a-z0-9._-]{0,62}[a-z0-9])?$")
    {
        return Err("RunnerClass built-in or custom identity drifted".to_string());
    }

    let bounds = [
        (
            "/$defs/RegisterRequest/properties/labels/maxItems",
            MAX_LABELS as u64,
        ),
        (
            "/$defs/RegisterRequest/properties/capacity/maximum",
            u64::from(MAX_CAPACITY),
        ),
        (
            "/$defs/WireJobRequest/properties/steps/maxItems",
            MAX_STEPS as u64,
        ),
        (
            "/$defs/WireJobResult/properties/artifact_digests/maxItems",
            MAX_RESULT_ITEMS as u64,
        ),
        (
            "/$defs/WireJobResult/properties/cache_receipts/maxItems",
            MAX_RESULT_ITEMS as u64,
        ),
    ];
    for (pointer, expected) in bounds {
        let actual = schema_u64(schema, pointer)?;
        if actual != expected {
            return Err(format!(
                "schema bound drift at {pointer}: expected {expected}, got {actual}"
            ));
        }
    }

    Ok(())
}

#[test]
fn checked_schema_matches_rust_wire_contract() {
    let schema: Value = serde_json::from_str(SCHEMA_TEXT).expect("schema must be valid JSON");
    validate_contract(&schema).expect("checked schema must match authoritative Rust wire types");
}

#[test]
fn drift_guard_rejects_hostile_schema_mutations() {
    let schema: Value = serde_json::from_str(SCHEMA_TEXT).expect("schema must be valid JSON");
    let mut variants = Vec::new();

    let mut missing_required = schema.clone();
    missing_required["$defs"]["RegisterRequest"]["required"]
        .as_array_mut()
        .expect("required array")
        .pop();
    variants.push(("missing required field", missing_required));

    let mut extra_property = schema.clone();
    extra_property["$defs"]["RegisterRequest"]["properties"]
        .as_object_mut()
        .expect("properties object")
        .insert(
            "access_token".to_string(),
            Value::String("forbidden".to_string()),
        );
    variants.push(("extra credential property", extra_property));

    let mut renamed_property = schema.clone();
    let properties = renamed_property["$defs"]["RunnerContext"]["properties"]
        .as_object_mut()
        .expect("properties object");
    let runner_id = properties.remove("runner_id").expect("runner_id property");
    properties.insert("runnerId".to_string(), runner_id);
    variants.push(("renamed field", renamed_property));

    let mut wrong_scalar_type = schema.clone();
    wrong_scalar_type["$defs"]["RunnerContext"]["properties"]["runner_epoch"] =
        serde_json::json!({"type": "string"});
    variants.push(("wrong scalar type", wrong_scalar_type));

    let mut swapped_object_type = schema.clone();
    swapped_object_type["$defs"]["HeartbeatRequest"]["properties"]["runner"] =
        serde_json::json!({"$ref": "#/$defs/ExecutionContext"});
    variants.push(("swapped object reference", swapped_object_type));

    let mut extra_message = schema;
    extra_message["oneOf"]
        .as_array_mut()
        .expect("oneOf array")
        .push(serde_json::json!({"$ref": "#/$defs/UnreviewedMessage"}));
    variants.push(("extra message", extra_message));

    for (name, variant) in variants {
        assert!(
            validate_contract(&variant).is_err(),
            "hostile mutation unexpectedly passed: {name}"
        );
    }
}
