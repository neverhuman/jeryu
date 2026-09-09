//! Validation error type and slug validation.

use std::fmt::{self, Display};

/// A generic validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    field: &'static str,
    message: &'static str,
}

impl ValidationError {
    /// Create a validation error.
    pub fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    /// Field that failed validation.
    pub fn field(&self) -> &'static str {
        self.field
    }

    /// Message for the failure.
    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validate slug-like identifiers.
pub fn validate_slug(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::new(field, "empty"));
    }
    if value.len() > 128 {
        return Err(ValidationError::new(field, "too long"));
    }
    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !ok {
        return Err(ValidationError::new(field, "invalid character"));
    }
    Ok(())
}
