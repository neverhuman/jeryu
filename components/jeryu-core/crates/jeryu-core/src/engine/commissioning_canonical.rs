//! Strict JSON input and canonical bytes for new commissioning contracts.
//! Existing journal hashes retain their historical serialization. This module
//! does not authenticate signatures, grant authority or perform any mutation.

use std::cell::Cell;
use std::fmt;
use std::io::Write;

use serde::de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};

use crate::{ForgeError, Result};

const MAX_INPUT: usize = 8 * 1024 * 1024;
const MAX_VALUES: usize = 100_000;
const MAX_DEPTH: usize = 64;
const MAX_INTEGER: i64 = 9_007_199_254_740_991;

struct StrictValue<'a> {
    remaining: &'a Cell<usize>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for StrictValue<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.depth > MAX_DEPTH || self.remaining.get() == 0 {
            return Err(serde::de::Error::custom(
                "commissioning JSON complexity exceeds bound",
            ));
        }
        self.remaining.set(self.remaining.get() - 1);
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictValue<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("strict commissioning JSON with bounded integer values")
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Value, E> {
        if !(-MAX_INTEGER..=MAX_INTEGER).contains(&value) {
            return Err(E::custom(
                "commissioning integer exceeds exact interoperable range",
            ));
        }
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Value, E> {
        if value > MAX_INTEGER as u64 {
            return Err(E::custom(
                "commissioning integer exceeds exact interoperable range",
            ));
        }
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Value, E> {
        Err(E::custom(
            "commissioning JSON requires integer number syntax",
        ))
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> std::result::Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(StrictValue {
            remaining: self.remaining,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> std::result::Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom(
                    "duplicate commissioning JSON member",
                ));
            }
            let value = object.next_value_seed(StrictValue {
                remaining: self.remaining,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

/// Parse duplicate-free JSON before the owning closed typed contract rejects
/// unknown fields, omitted identities, unsupported roles and malformed values.
/// Floats/exponents, negative zero and integers outside ±(2^53-1) are refused.
pub(super) fn parse_contract<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT {
        return Err(ForgeError::Validation(
            "commissioning JSON input size is unsupported".into(),
        ));
    }
    let remaining = Cell::new(MAX_VALUES);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue {
        remaining: &remaining,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| ForgeError::Validation(format!("invalid commissioning JSON: {error}")))?;
    deserializer
        .end()
        .map_err(|error| ForgeError::Validation(format!("trailing commissioning JSON: {error}")))?;
    if !value.is_object() {
        return Err(ForgeError::Validation(
            "commissioning contract must be an object".into(),
        ));
    }
    serde_json::from_value(value).map_err(|error| {
        ForgeError::Validation(format!("invalid typed commissioning contract: {error}"))
    })
}

/// RFC8785 canonical representation restricted to the admitted integer-only
/// profile. Object keys are ordered by UTF-16 code units, not UTF-8 bytes.
/// Arrays keep their declared order; strings are not Unicode-normalized.
pub(super) fn canonical_payload<T: Serialize>(payload: &T) -> Result<Vec<u8>> {
    struct BoundedBytes(Vec<u8>);
    impl Write for BoundedBytes {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_INPUT.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other(
                    "commissioning serialization exceeds byte bound",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = BoundedBytes(Vec::new());
    serde_json::to_writer(&mut bytes, payload)
        .map_err(|error| ForgeError::Validation(format!("commissioning serialization: {error}")))?;
    let value = parse_contract::<Value>(&bytes.0)?;
    let mut result = Vec::new();
    let mut remaining = MAX_VALUES;
    encode(&value, &mut result, 0, &mut remaining)?;
    Ok(result)
}

fn encode(value: &Value, output: &mut Vec<u8>, depth: usize, remaining: &mut usize) -> Result<()> {
    if depth > MAX_DEPTH || *remaining == 0 {
        return Err(ForgeError::Validation(
            "commissioning JSON complexity exceeds bound".into(),
        ));
    }
    *remaining -= 1;
    match value {
        Value::Object(object) => {
            let mut members = object.iter().collect::<Vec<_>>();
            members.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            output.push(b'{');
            for (index, (key, value)) in members.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                serde_json::to_writer(&mut *output, key)
                    .map_err(|error| ForgeError::Validation(error.to_string()))?;
                output.push(b':');
                encode(value, output, depth + 1, remaining)?;
            }
            output.push(b'}');
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                encode(value, output, depth + 1, remaining)?;
            }
            output.push(b']');
        }
        Value::Number(number) => {
            let valid = number
                .as_i64()
                .is_some_and(|n| (-MAX_INTEGER..=MAX_INTEGER).contains(&n))
                || number.as_u64().is_some_and(|n| n <= MAX_INTEGER as u64);
            if !valid {
                return Err(ForgeError::Validation(
                    "unsupported commissioning number".into(),
                ));
            }
            output.extend_from_slice(number.to_string().as_bytes());
        }
        other => serde_json::to_writer(&mut *output, other)
            .map_err(|error| ForgeError::Validation(error.to_string()))?,
    }
    if output.len() > MAX_INPUT {
        return Err(ForgeError::Validation(
            "canonical commissioning payload exceeds byte bound".into(),
        ));
    }
    Ok(())
}

/// Hash only the unsigned typed payload: schema UTF-8, one NUL byte, then the
/// canonical payload bytes. Detached acceptance records/signatures and the
/// envelope's claimed digest must stay outside this payload to avoid a cycle.
pub(super) fn payload_sha256<T: Serialize>(schema: &str, payload: &T) -> Result<String> {
    if schema.is_empty()
        || schema.len() > 128
        || !schema
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"./-".contains(&b))
    {
        return Err(ForgeError::Validation(
            "invalid commissioning schema domain".into(),
        ));
    }
    let mut digest = Sha256::new();
    digest.update(schema.as_bytes());
    digest.update([0]);
    digest.update(canonical_payload(payload)?);
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Window {
        expires: u64,
        enabled: bool,
    }

    #[test]
    fn duplicate_decoded_keys_refuse_at_every_depth() {
        for input in [
            r#"{"issuer":1,"issuer":2}"#,
            r#"{"issuer":1,"\u0069ssuer":2}"#,
            r#"{"role":{"id":1,"id":2}}"#,
            r#"{"roles":[{"id":1,"id":2}]}"#,
        ] {
            assert!(
                parse_contract::<Value>(input.as_bytes())
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate")
            );
        }
    }

    #[test]
    fn typed_fields_refuse_coercion_unknown_omitted_and_trailing_values() {
        assert_eq!(
            parse_contract::<Window>(br#"{"enabled":false,"expires":7200}"#).unwrap(),
            Window {
                expires: 7200,
                enabled: false
            }
        );
        for input in [
            r#"{"expires":"7200","enabled":false}"#,
            r#"{"expires":true,"enabled":false}"#,
            r#"{"expires":1.0,"enabled":false}"#,
            r#"{"expires":1e2,"enabled":false}"#,
            r#"{"expires":-0,"enabled":false}"#,
            r#"{"expires":7200,"enabled":false,"other":1}"#,
            r#"{"expires":7200}"#,
            r#"{"expires":7200,"enabled":false}{}"#,
        ] {
            assert!(
                parse_contract::<Window>(input.as_bytes()).is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn integer_boundaries_and_programmatic_floats_are_exact() {
        for value in [-MAX_INTEGER, 0, MAX_INTEGER] {
            let bytes = format!("{{\"value\":{value}}}");
            assert!(parse_contract::<Value>(bytes.as_bytes()).is_ok());
            assert_eq!(
                canonical_payload(&json!({"value":value})).unwrap(),
                bytes.as_bytes()
            );
        }
        for value in [i64::MIN, -MAX_INTEGER - 1, MAX_INTEGER + 1, i64::MAX] {
            let bytes = format!("{{\"value\":{value}}}");
            assert!(parse_contract::<Value>(bytes.as_bytes()).is_err());
            assert!(canonical_payload(&json!({"value":value})).is_err());
        }
        assert!(canonical_payload(&json!({"value":1.0})).is_err());
    }

    #[test]
    fn utf16_key_order_strings_arrays_and_escaping_are_canonical() {
        let first = parse_contract::<Value>(
            "{\"\u{e000}\":1,\"\u{10000}\":2,\"a\":[2,1],\"s\":\"é\\n\"}".as_bytes(),
        )
        .unwrap();
        let expected = "{\"a\":[2,1],\"s\":\"é\\n\",\"\u{10000}\":2,\"\u{e000}\":1}";
        assert_eq!(canonical_payload(&first).unwrap(), expected.as_bytes());
        assert_ne!(
            canonical_payload(&json!({"s":"é"})).unwrap(),
            canonical_payload(&json!({"s":"e\u{301}"})).unwrap()
        );
    }

    #[test]
    fn schema_domain_and_field_values_change_unsigned_hash() {
        let value = json!({"expires":7200,"operation":"restore"});
        let a = payload_sha256("jeryu.forge-staged-commissioning/v1", &value).unwrap();
        assert_eq!(
            a,
            payload_sha256(
                "jeryu.forge-staged-commissioning/v1",
                &json!({"operation":"restore","expires":7200})
            )
            .unwrap()
        );
        assert_ne!(
            a,
            payload_sha256("jeryu.forge-staged-commissioning/v2", &value).unwrap()
        );
        assert_ne!(
            a,
            payload_sha256(
                "jeryu.forge-staged-commissioning/v1",
                &json!({"expires":7199,"operation":"restore"})
            )
            .unwrap()
        );
        assert!(payload_sha256("bad\0domain", &value).is_err());
    }

    #[test]
    fn fixed_independent_unsigned_hash_domain_answer_matches() {
        let payload = json!({
            "origin":"reviewed_staging_candidate",
            "expires_at":7200,
            "contract_id":"00000000-0000-4000-8000-000000000001"
        });
        assert_eq!(
            canonical_payload(&payload).unwrap(),
            br#"{"contract_id":"00000000-0000-4000-8000-000000000001","expires_at":7200,"origin":"reviewed_staging_candidate"}"#
        );
        assert_eq!(
            payload_sha256("jeryu.forge-staged-commissioning/v1", &payload).unwrap(),
            "f7e2ee3b593bcda180729c2222b007c57ff069dbb7e71e98a9d4727e4196b756"
        );
    }

    #[test]
    fn malformed_utf8_and_unpaired_surrogates_refuse_without_replacement() {
        for bytes in [
            b"{\"value\":\"\xff\"}".as_slice(),
            br#"{"value":"\ud800"}"#.as_slice(),
            br#"{"value":"\udfff"}"#.as_slice(),
            br#"{"value":"\ud800x"}"#.as_slice(),
            br#"{"\ud800":true}"#.as_slice(),
        ] {
            assert!(parse_contract::<Value>(bytes).is_err());
        }
        let paired = parse_contract::<Value>(br#"{"value":"\ud800\udc00"}"#).unwrap();
        assert_eq!(paired["value"], "\u{10000}");
        assert_eq!(
            canonical_payload(&paired).unwrap(),
            "{\"value\":\"\u{10000}\"}".as_bytes()
        );
    }

    #[test]
    fn byte_depth_and_value_count_limits_refuse() {
        assert!(parse_contract::<Value>(&vec![b' '; MAX_INPUT + 1]).is_err());
        let input = format!(
            "{{\"deep\":{}0{}}}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert!(parse_contract::<Value>(input.as_bytes()).is_err());
        let input = format!("{{\"values\":[{}]}}", vec!["0"; MAX_VALUES].join(","));
        assert!(parse_contract::<Value>(input.as_bytes()).is_err());
        assert!(canonical_payload(&json!({"values":vec![0;MAX_VALUES]})).is_err());
    }
}
