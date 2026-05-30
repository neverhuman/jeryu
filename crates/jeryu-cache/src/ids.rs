//! Validated identity newtypes used by policy and receipts.

use crate::error::{Result, VaultError};
use std::fmt::{Display, Formatter};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Creates a validated identifier.
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(VaultError::InvalidInput(format!(
                        "{} cannot be empty",
                        stringify!($name)
                    )));
                }
                if value.contains('\n') || value.contains('\r') || value.contains('\0') {
                    return Err(VaultError::InvalidInput(format!(
                        "{} contains a forbidden control character",
                        stringify!($name)
                    )));
                }
                Ok(Self(value))
            }

            /// Returns the string value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(TenantId);
id_type!(RepoId);
id_type!(Actor);
id_type!(SharedScopeId);
