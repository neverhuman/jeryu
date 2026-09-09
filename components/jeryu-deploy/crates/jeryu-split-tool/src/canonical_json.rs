//! Stable JSON object order across package and workspace feature unification.

use serde_json::Value;

pub(super) fn pretty(mut value: Value) -> serde_json::Result<String> {
    value.sort_all_objects();
    serde_json::to_string_pretty(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_objects_are_sorted_without_reordering_arrays() {
        let value = serde_json::from_str(r#"{"z":[{"z":1,"a":2},0],"a":{"z":3,"a":4}}"#).unwrap();
        assert_eq!(
            pretty(value).unwrap(),
            "{\n  \"a\": {\n    \"a\": 4,\n    \"z\": 3\n  },\n  \"z\": [\n    {\n      \"a\": 2,\n      \"z\": 1\n    },\n    0\n  ]\n}"
        );
    }
}
