//! Backup and restore drills.

/// Restore invariant required after a backup drill.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreInvariant {
    pub name: String,
    pub passed: bool,
}

/// Backup drill receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupDrill {
    pub backup_id: String,
    pub tenant: String,
    pub encrypted: bool,
    pub object_count: u64,
    pub restored_object_count: u64,
    pub invariants: Vec<RestoreInvariant>,
}

impl BackupDrill {
    /// Construct a valid backup drill.
    pub fn successful(tenant: &str, object_count: u64) -> Self {
        Self {
            backup_id: format!("backup-{tenant}-{object_count}"),
            tenant: tenant.to_owned(),
            encrypted: true,
            object_count,
            restored_object_count: object_count,
            invariants: vec![
                RestoreInvariant {
                    name: "repos-restored".to_owned(),
                    passed: true,
                },
                RestoreInvariant {
                    name: "receipts-restored".to_owned(),
                    passed: true,
                },
                RestoreInvariant {
                    name: "audit-chain-valid".to_owned(),
                    passed: true,
                },
            ],
        }
    }

    /// True when the restored data is operationally usable.
    pub fn passes(&self) -> bool {
        self.encrypted
            && self.object_count == self.restored_object_count
            && self.object_count > 0
            && self.invariants.iter().all(|invariant| invariant.passed)
    }
}
