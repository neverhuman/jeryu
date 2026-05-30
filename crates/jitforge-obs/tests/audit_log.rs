use jitforge_obs::AuditLog;

#[test]
fn audit_log_is_hash_chained_and_append_only() {
    let mut log = AuditLog::new();
    log.append("alice", "benchmark.publish", "tenant-a", "bench_1", "allow");
    log.append("bob", "tenant.delete", "tenant-a", "tenant-a", "deny");
    assert!(log.verify());
    assert_eq!(log.events()[0].previous_hash, "genesis");
    assert_eq!(log.events()[1].previous_hash, log.events()[0].event_hash);
}
