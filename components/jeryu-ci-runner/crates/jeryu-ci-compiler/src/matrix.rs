//! Build-matrix parsing, the cartesian expansion of matrix combinations, and
//! deterministic id/name derivation for each expanded job.

use std::collections::BTreeMap;

use jeryu_ci_ir::sanitize_id;

use crate::lexer::{SourceLine, parse_array_or_scalar};

pub(crate) fn parse_matrix(lines: &[SourceLine]) -> BTreeMap<String, Vec<String>> {
    let mut matrix = BTreeMap::new();
    let mut inside_matrix = false;
    let mut matrix_indent = 0;
    for line in lines {
        if line.text == "matrix:" {
            inside_matrix = true;
            matrix_indent = line.indent;
            continue;
        }
        if inside_matrix
            && line.indent > matrix_indent
            && let Some((key, value)) = line.text.split_once(':')
        {
            matrix.insert(key.trim().to_string(), parse_array_or_scalar(value.trim()));
        }
    }
    matrix
}

pub(crate) fn matrix_combinations(
    matrix: &BTreeMap<String, Vec<String>>,
) -> Vec<BTreeMap<String, String>> {
    if matrix.is_empty() {
        return vec![BTreeMap::new()];
    }
    let mut combos = vec![BTreeMap::new()];
    for (key, values) in matrix {
        let mut next = Vec::new();
        for combo in &combos {
            for value in values {
                let mut expanded = combo.clone();
                expanded.insert(key.clone(), value.clone());
                next.push(expanded);
            }
        }
        combos = next;
    }
    combos
}

pub(crate) fn expanded_id(origin: &str, combo: &BTreeMap<String, String>) -> String {
    if combo.is_empty() {
        return sanitize_id(origin);
    }
    let suffix = combo
        .iter()
        .map(|(k, v)| format!("{k}_{v}"))
        .collect::<Vec<_>>()
        .join("__");
    sanitize_id(&format!("{origin}__{suffix}"))
}

pub(crate) fn expanded_name(origin: &str, combo: &BTreeMap<String, String>) -> String {
    if combo.is_empty() {
        origin.to_string()
    } else {
        let suffix = combo
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{origin} ({suffix})")
    }
}
