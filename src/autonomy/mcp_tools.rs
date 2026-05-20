//! MCP tool descriptors for Evidence Gate operations.
//!
//! Returns JSON-schema-shaped tool descriptors that can be folded into the
//! existing `src/mcp/tools.rs::tool_manifest()` when Codex's Phase 9 wires
//! the MCP server to expose them. Until then these descriptors can be
//! consumed via `jeryu::autonomy::mcp_tools::descriptors()` and serialized
//! directly by anyone integrating MCP externally.
//!
//! Tool catalog (Phase 9 minimum, read-only):
//!   - `vibegate.inspect_autonomy_pack` — return the parsed PolicyBundle for a repo.
//!   - `vibegate.get_evidence_pack` — fetch an EvidencePack by id (Codex's DB).
//!   - `vibegate.get_verdict` — fetch a VibeGateVerdict by id.
//!   - `vibegate.list_receipts` — list AgentApprovalReceipts for an EvidencePack.
//!   - `vibegate.get_agent_health` — last N agent runs + p50/p95 latency.
//!   - `vibegate.doctor` — provider sweep (live; lease-gated for non-read-only intent).
//!
//! Write tools (lease-gated; land in a later phase):
//!   - `vibegate.run_review` — kick off a reviewer call.
//!   - `vibegate.approve_mr` — judge → host approval.
//!   - `vibegate.propose_autonomy_edit` — open an MR against `.jeryu/autonomy/`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub input_schema: serde_json::Value,
    pub requires_lease: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    ReadOnly,
    Mutating,
}

pub fn descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "vibegate.inspect_autonomy_pack".into(),
            description: "Load and return the parsed .jeryu/autonomy PolicyBundle for the current repo.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "autonomy_dir": { "type": "string", "default": ".jeryu/autonomy" }
                }
            }),
        },
        ToolDescriptor {
            name: "vibegate.get_evidence_pack".into(),
            description: "Fetch a stored EvidencePack by id.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": { "id": { "type": "string", "pattern": "^evp_[0-9A-Z]+$" } }
            }),
        },
        ToolDescriptor {
            name: "vibegate.get_verdict".into(),
            description: "Fetch a VibeGateVerdict by id.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["id"],
                "properties": { "id": { "type": "string", "pattern": "^vgv_[0-9A-Z]+$" } }
            }),
        },
        ToolDescriptor {
            name: "vibegate.list_receipts".into(),
            description: "List AgentApprovalReceipts attached to an EvidencePack.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["evidence_pack_id"],
                "properties": { "evidence_pack_id": { "type": "string" } }
            }),
        },
        ToolDescriptor {
            name: "vibegate.get_agent_health".into(),
            description: "Return the last N agent runs with p50/p95 latency and error rate.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "since_seconds": { "type": "integer", "default": 3600 },
                    "limit": { "type": "integer", "default": 50 }
                }
            }),
        },
        ToolDescriptor {
            name: "vibegate.doctor".into(),
            description: "Probe every configured LLM provider; return OK / AUTH / RATE / DOWN per provider. Live network call.".into(),
            category: ToolCategory::ReadOnly,
            requires_lease: false,
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        ToolDescriptor {
            name: "vibegate.run_review".into(),
            description: "Kick off one reviewer role against a diff. Lease-gated; the diff must be scrubbed first.".into(),
            category: ToolCategory::Mutating,
            requires_lease: true,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["role", "evidence_pack_id", "diff"],
                "properties": {
                    "role": { "enum": ["security", "test_integrity", "runtime", "lockfile"] },
                    "evidence_pack_id": { "type": "string" },
                    "diff": { "type": "string" }
                }
            }),
        },
        ToolDescriptor {
            name: "vibegate.approve_mr".into(),
            description: "Post a SHA-bound approval check-run on the merge request. Lease-gated.".into(),
            category: ToolCategory::Mutating,
            requires_lease: true,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["repo", "mr_iid", "head_sha", "agent_id", "receipt_digest"],
                "properties": {
                    "repo": { "type": "string", "pattern": "^[^/]+/[^/]+$" },
                    "mr_iid": { "type": "string" },
                    "head_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
                    "agent_id": { "type": "string" },
                    "receipt_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" }
                }
            }),
        },
        ToolDescriptor {
            name: "vibegate.propose_autonomy_edit".into(),
            description: "Open an MR proposing changes to `.jeryu/autonomy/`. Never direct-writes; always goes through review per Tip1 Law 3.".into(),
            category: ToolCategory::Mutating,
            requires_lease: true,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["path", "new_content", "rationale"],
                "properties": {
                    "path": { "type": "string", "pattern": "^\\.jeryu/autonomy/" },
                    "new_content": { "type": "string" },
                    "rationale": { "type": "string", "maxLength": 1000 }
                }
            }),
        },
    ]
}

/// Categorize descriptors by mutation requirement (useful for MCP grant scoping).
pub fn read_only() -> Vec<ToolDescriptor> {
    descriptors()
        .into_iter()
        .filter(|d| d.category == ToolCategory::ReadOnly)
        .collect()
}

pub fn mutating() -> Vec<ToolDescriptor> {
    descriptors()
        .into_iter()
        .filter(|d| d.category == ToolCategory::Mutating)
        .collect()
}

impl ToolDescriptor {
    /// Convert to the JSON shape that MCP `tool_manifest()` returns. Shape:
    /// `{ name, title, description, inputSchema, outputSchema, annotations }`.
    pub fn to_mcp_json(&self) -> serde_json::Value {
        let read_only = self.category == ToolCategory::ReadOnly;
        serde_json::json!({
            "name": self.name,
            "title": pretty_title(&self.name),
            "description": self.description,
            "inputSchema": self.input_schema,
            "outputSchema": serde_json::json!({ "type": "object" }),
            "annotations": {
                "readOnlyHint": read_only,
                "destructiveHint": false,
                "idempotentHint": read_only,
                "openWorldHint": !read_only,
                "leaseRequired": self.requires_lease,
            },
        })
    }
}

fn pretty_title(name: &str) -> String {
    name.strip_prefix("vibegate.")
        .unwrap_or(name)
        .replace('_', " ")
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convenience: return every autonomy descriptor as MCP-shaped JSON.
/// Slots cleanly into `src/mcp/tools.rs::tool_manifest()`.
pub fn manifest_jsons() -> Vec<serde_json::Value> {
    descriptors().iter().map(|d| d.to_mcp_json()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_catalog_has_9_tools() {
        let d = descriptors();
        assert_eq!(
            d.len(),
            9,
            "Phase 9 tool count should be exactly 9 (6 read-only + 3 mutating)"
        );
    }

    #[test]
    fn read_only_and_mutating_partition_the_catalog() {
        let ro = read_only();
        let mu = mutating();
        assert_eq!(ro.len(), 6);
        assert_eq!(mu.len(), 3);
        // Every mutating tool requires a lease.
        for d in &mu {
            assert!(
                d.requires_lease,
                "mutating tool {} must require_lease",
                d.name
            );
        }
        // No read-only tool requires a lease.
        for d in &ro {
            assert!(
                !d.requires_lease,
                "read-only tool {} must not require_lease",
                d.name
            );
        }
    }

    #[test]
    fn all_descriptors_use_vibegate_prefix() {
        for d in descriptors() {
            assert!(
                d.name.starts_with("vibegate."),
                "tool {} must use vibegate. prefix",
                d.name
            );
        }
    }

    #[test]
    fn input_schemas_have_valid_json() {
        for d in descriptors() {
            assert!(
                d.input_schema.is_object(),
                "tool {} schema must be object",
                d.name
            );
            assert_eq!(
                d.input_schema["type"].as_str(),
                Some("object"),
                "tool {} schema.type must be 'object'",
                d.name
            );
        }
    }

    #[test]
    fn descriptors_round_trip_through_serde() {
        let d = descriptors();
        let json = serde_json::to_string(&d).unwrap();
        let back: Vec<ToolDescriptor> = serde_json::from_str(&json).unwrap();
        assert_eq!(d.len(), back.len());
        for (a, b) in d.iter().zip(back.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.category, b.category);
            assert_eq!(a.requires_lease, b.requires_lease);
        }
    }
}
