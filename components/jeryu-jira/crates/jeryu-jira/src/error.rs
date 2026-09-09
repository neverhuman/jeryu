use serde::Serialize;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, WorkError>;

#[derive(Debug, Error)]
pub enum WorkError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("work item not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WorkRepairHint {
    pub code: &'static str,
    pub purpose: &'static str,
    pub reason: &'static str,
    pub common_fixes: &'static [&'static str],
    pub docs_url: &'static str,
    pub repair_hint: &'static str,
}

impl WorkError {
    pub(crate) fn storage(error: impl std::fmt::Display) -> Self {
        Self::Storage(error.to_string())
    }

    pub(crate) fn serialization(error: impl std::fmt::Display) -> Self {
        Self::Serialization(error.to_string())
    }

    /// Stable machine code for routing a failed Work operation without parsing
    /// human-readable or potentially sensitive error details.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "work.validation",
            Self::NotFound(_) => "work.not_found",
            Self::Conflict(_) => "work.conflict",
            Self::Storage(_) => "work.storage",
            Self::Serialization(_) => "work.serialization",
        }
    }

    /// Static, serializable repair guidance for machine-readable `json diagnostics`
    /// and receipts.
    ///
    /// The returned value deliberately excludes the variant's owned message;
    /// callers that need receipt-safe diagnostics must serialize this hint rather
    /// than [`WorkError::to_string`].
    ///
    /// ```
    /// use jeryu_jira::{WorkError, WorkRepairHint};
    ///
    /// let error = WorkError::Storage("private backend detail".to_string());
    /// let hint: WorkRepairHint = error.repair_hint();
    /// let diagnostic = serde_json::to_value(hint)?;
    /// assert_eq!(diagnostic["code"], "work.storage");
    /// assert!(!diagnostic.to_string().contains("private backend detail"));
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    #[must_use]
    pub fn repair_hint(&self) -> WorkRepairHint {
        let code = self.code();
        match self {
            Self::Validation(_) => WorkRepairHint {
                code,
                purpose: "repair Work request validation",
                reason: "the request crossed a Work boundary with an empty field or non-positive issue or pull request number",
                common_fixes: &[
                    "trim titles, comments, owners, and repo names before retrying",
                    "send Work issue and pull request numbers greater than zero",
                    "rerun `rtk just check` and the affected API or web lane",
                ],
                docs_url: "docs/testing.md#repair-evidence",
                repair_hint: "fix the request payload, then rerun the narrow Work proof lane named in agent/test-map.json",
            },
            Self::NotFound(_) => WorkRepairHint {
                code,
                purpose: "repair missing Work item lookup",
                reason: "the requested Work key does not resolve to a persisted item",
                common_fixes: &[
                    "verify the key has the JRY-<number> shape",
                    "list Work items for the repo before linking or commenting",
                    "rerun `rtk just migration-drift` if the database was recreated",
                ],
                docs_url: "docs/testing.md#repair-evidence",
                repair_hint: "load the Work list first and retry with an existing key",
            },
            Self::Conflict(_) => WorkRepairHint {
                code,
                purpose: "repair Work link conflict",
                reason: "a GitHub-compatible issue link is already owned by another Work item",
                common_fixes: &[
                    "call find_by_issue before creating a mirrored Work item",
                    "reuse the existing Work item instead of creating a duplicate",
                    "rerun `rtk just migration-drift` to prove the uniqueness constraint",
                ],
                docs_url: "docs/boundaries.md#data-boundary",
                repair_hint: "preserve one Work item per mirrored issue and attach new evidence to the existing item",
            },
            Self::Storage(_) => WorkRepairHint {
                code,
                purpose: "repair Work storage failure",
                reason: "SQLite rejected or could not persist the Work operation",
                common_fixes: &[
                    "check the SQLite path and parent directory permissions",
                    "rerun `rtk just migration-drift` for schema or constraint failures",
                    "inspect target/jankurai/migration-report.json when migration analysis ran",
                ],
                docs_url: "docs/boundaries.md#data-boundary",
                repair_hint: "fix the storage condition, then rerun migration drift and the caller integration test",
            },
            Self::Serialization(_) => WorkRepairHint {
                code,
                purpose: "repair Work row decoding",
                reason: "stored JSON, timestamps, or enum strings no longer match the Work contract",
                common_fixes: &[
                    "regenerate contracts with `rtk cargo run -p jeryu-jira --bin jeryu-jira-export-contracts`",
                    "rerun `rtk just contract-drift`",
                    "repair malformed rows through a reviewed migration, not ad hoc SQL",
                ],
                docs_url: "docs/testing.md#repair-evidence",
                repair_hint: "restore contract/schema alignment before retrying the caller",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::WorkError;

    #[test]
    fn repair_hints_have_unique_stable_codes_and_complete_guidance() {
        let cases = [
            (WorkError::Validation(String::new()), "work.validation"),
            (WorkError::NotFound(String::new()), "work.not_found"),
            (WorkError::Conflict(String::new()), "work.conflict"),
            (WorkError::Storage(String::new()), "work.storage"),
            (
                WorkError::Serialization(String::new()),
                "work.serialization",
            ),
        ];
        let mut codes = BTreeSet::new();

        for (error, expected_code) in cases {
            let hint = error.repair_hint();
            assert_eq!(error.code(), expected_code);
            assert_eq!(hint.code, expected_code);
            assert!(codes.insert(hint.code), "duplicate code: {}", hint.code);
            assert!(!hint.purpose.is_empty());
            assert!(!hint.reason.is_empty());
            assert!(!hint.common_fixes.is_empty());
            assert!(!hint.docs_url.is_empty());
            assert!(!hint.repair_hint.is_empty());
        }
    }

    #[test]
    fn serialized_repair_hint_has_exact_shape_and_excludes_raw_error_detail() {
        let private_detail = "storage-detail-canary-7fcb0f9d";
        let error = WorkError::Storage(private_detail.to_string());
        assert!(error.to_string().contains(private_detail));

        let value = serde_json::to_value(error.repair_hint()).expect("serialize repair hint");
        let object = value.as_object().expect("repair hint is an object");
        let actual_keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        let expected_keys = [
            "code",
            "common_fixes",
            "docs_url",
            "purpose",
            "reason",
            "repair_hint",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();

        assert_eq!(actual_keys, expected_keys);
        assert_eq!(value["code"], "work.storage");
        assert!(
            value["common_fixes"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );
        assert!(!value.to_string().contains(private_detail));
    }
}
