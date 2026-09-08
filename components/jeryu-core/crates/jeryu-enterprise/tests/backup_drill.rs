use jeryu_enterprise::BackupDrill;

#[test]
fn backup_restore_drill_preserves_invariants() {
    let drill = BackupDrill::successful("tenant-a", 42);
    assert!(drill.passes());
}
