use redlinedb::Database;

#[test]
fn jeryu_work_and_audit_state_survives_transactions_and_reopen() {
    let dir = tempfile::tempdir().expect("temporary database directory");
    let path = dir.path().join("jeryu-consumer.redline");

    {
        let database = Database::create(&path).expect("create Redline database");
        let mut connection = database.connect().expect("connect to Redline database");

        connection
            .execute(
                "CREATE TABLE work_items(\
                    id TEXT PRIMARY KEY,\
                    tenant TEXT NOT NULL,\
                    status TEXT NOT NULL,\
                    version INTEGER NOT NULL\
                )",
                (),
            )
            .expect("create Jeryu work-item table");
        connection
            .execute(
                "CREATE TABLE audit_receipts(\
                    id TEXT PRIMARY KEY,\
                    tenant TEXT NOT NULL,\
                    subject TEXT NOT NULL,\
                    payload TEXT NOT NULL\
                )",
                (),
            )
            .expect("create Jeryu audit-receipt table");

        connection
            .execute(
                "INSERT INTO work_items(id, tenant, status, version) VALUES (?, ?, ?, ?)",
                ("JRY-800", "tenant-a", "ready", 1_i64),
            )
            .expect("insert work item");

        connection
            .execute("BEGIN IMMEDIATE", ())
            .expect("begin durable update");
        connection
            .execute(
                "UPDATE work_items SET status = ?, version = ? WHERE id = ? AND tenant = ?",
                ("released", 2_i64, "JRY-800", "tenant-a"),
            )
            .expect("update work item");
        connection
            .execute(
                "INSERT INTO audit_receipts(id, tenant, subject, payload) VALUES (?, ?, ?, ?)",
                (
                    "receipt-800",
                    "tenant-a",
                    "JRY-800",
                    "{\"status\":\"released\",\"version\":2}",
                ),
            )
            .expect("insert audit receipt");
        connection.execute("COMMIT", ()).expect("commit update");

        connection
            .execute("BEGIN IMMEDIATE", ())
            .expect("begin rollback probe");
        connection
            .execute(
                "UPDATE work_items SET status = ? WHERE id = ?",
                ("corrupted", "JRY-800"),
            )
            .expect("stage rollback probe");
        connection.execute("ROLLBACK", ()).expect("rollback probe");

        let status: String = connection
            .query_row(
                "SELECT status FROM work_items WHERE id = ? AND tenant = ?",
                ("JRY-800", "tenant-a"),
            )
            .expect("read committed status");
        assert_eq!(status, "released");

        database
            .checkpoint()
            .expect("checkpoint durable Jeryu consumer state");
    }

    let reopened = Database::open(&path).expect("reopen Redline database");
    let mut connection = reopened.connect().expect("connect after reopen");
    let version: i64 = connection
        .query_row(
            "SELECT version FROM work_items WHERE id = ? AND tenant = ?",
            ("JRY-800", "tenant-a"),
        )
        .expect("read durable work item");
    let receipt_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_receipts WHERE subject = ? AND tenant = ?",
            ("JRY-800", "tenant-a"),
        )
        .expect("read durable audit receipt");

    assert_eq!(version, 2);
    assert_eq!(receipt_count, 1);
}
