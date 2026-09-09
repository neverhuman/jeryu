//! Schema tag machinery (phantom type that serializes to a canonical schema id).
//!
//! `SchemaTag<T>` is a zero-size phantom that serializes to and validates against
//! a canonical schema id (`T::NAME`), letting each wire object carry a stable
//! `"schema"` discriminator without a runtime field.

use serde::{Deserialize, Serialize};

pub trait SchemaKind {
    const NAME: &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EvidencePackTag;
impl SchemaKind for EvidencePackTag {
    const NAME: &'static str = "vibegate.evidence_pack.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AgentApprovalReceiptTag;
impl SchemaKind for AgentApprovalReceiptTag {
    const NAME: &'static str = "vibegate.agent_approval_receipt.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VibeGateVerdictTag;
impl SchemaKind for VibeGateVerdictTag {
    const NAME: &'static str = "vibegate.gate_verdict.v1";
}

pub struct SchemaTag<T: SchemaKind>(std::marker::PhantomData<T>);

impl<T: SchemaKind> Default for SchemaTag<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}
impl<T: SchemaKind> Clone for SchemaTag<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: SchemaKind> Copy for SchemaTag<T> {}
impl<T: SchemaKind> std::fmt::Debug for SchemaTag<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SchemaTag<{}>", T::NAME)
    }
}
impl<T: SchemaKind> PartialEq for SchemaTag<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T: SchemaKind> Eq for SchemaTag<T> {}
impl<T: SchemaKind> std::hash::Hash for SchemaTag<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        T::NAME.hash(state);
    }
}
impl<T: SchemaKind> SchemaTag<T> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}
impl<T: SchemaKind> Serialize for SchemaTag<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(T::NAME)
    }
}
impl<'de, T: SchemaKind> Deserialize<'de> for SchemaTag<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s == T::NAME {
            Ok(Self::new())
        } else {
            Err(serde::de::Error::custom(format!(
                "expected schema tag {}, got {s}",
                T::NAME
            )))
        }
    }
}
