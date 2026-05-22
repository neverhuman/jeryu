use cargo_vrc::ReportRepairHint;
use serde::{Deserialize, Serialize};

mod helpers;
mod records;
mod report;
mod scan;

pub(crate) use helpers::*;
pub use records::{incomplete_records, init_records};
pub use report::{markdown_report, sarif_report};
pub use scan::scan_workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub class_id: String,
    pub severity: String,
    pub confidence: f64,
    pub path: String,
    pub summary: String,
    pub suggested_fix: String,
    #[serde(default)]
    pub existing_exception: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub generated_at: String,
    pub workspace_root: String,
    pub findings: Vec<Finding>,
    pub repair_hint: ReportRepairHint,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AerRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub class_id: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub exception: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub doc_links: Vec<String>,
    #[serde(default)]
    pub sunset_condition: String,
}
