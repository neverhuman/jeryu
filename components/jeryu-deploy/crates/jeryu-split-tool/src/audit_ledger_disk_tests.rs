use super::*;

fn disk_fixture(test: impl FnOnce(&Path)) {
    let root = tempfile::Builder::new()
        .prefix("jeryu-ledger-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
        .keep();
    eprintln!(
        "audit-ledger disk fixture retained for independent custody review: {}",
        root.display()
    );
    test(&root);
    // The ledger never proves process/mount custody. Do not remove even successful
    // fixture databases without the separately governed whole-root cleanup gate.
}

#[test]
fn durable_reopen_preserves_failures_read_only_status_and_append_only_rows() {
    disk_fixture(|root| {
        let path = root.join("ledger.sqlite");
        let mut fixture = Fixture::new();
        fixture.connection = open(&path, false).unwrap();
        fixture.import().unwrap();
        let start = fixture.start();
        let receipt = encoded(&fixture.receipt(&start, "source_unavailable", None, None));
        store::finish(&mut fixture.connection, &receipt, None, 111).unwrap();
        fixture.close(&start, 112).unwrap();
        let original = fixture.status();
        drop(fixture);
        let connection = open(&path, true).unwrap();
        assert_eq!(store::status(&connection).unwrap(), original);
        assert!(connection.execute("DELETE FROM observations", []).is_err());
        drop(connection);
        assert!(run(&path, Operation::Status).is_err());
        let connection = open(&path, false).unwrap();
        assert!(
            connection
                .execute(
                    "UPDATE observations SET outcome='completed_unqualified'",
                    []
                )
                .is_err()
        );
        assert!(connection.execute("DELETE FROM attempts", []).is_err());
        assert_eq!(store::status(&connection).unwrap(), original);
    });
}

#[test]
fn simultaneous_starts_get_only_one_live_lease() {
    disk_fixture(|root| {
        let path = root.join("ledger.sqlite");
        let mut fixture = Fixture::new();
        fixture.connection = open(&path, false).unwrap();
        fixture.import().unwrap();
        let request = fixture.plan["jobs"][0]["attempt_key"]
            .as_str()
            .unwrap()
            .to_owned();
        drop(fixture);
        let connections = [open(&path, false).unwrap(), open(&path, false).unwrap()];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = connections
            .into_iter()
            .map(|mut connection| {
                let request = request.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store::start(&mut connection, &request, 10, 110).is_ok()
                })
            })
            .collect();
        let successes = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|success| *success)
            .count();
        assert_eq!(successes, 1);
        let connection = open(&path, true).unwrap();
        assert_eq!(
            store::status(&connection).unwrap()["jobs"][0]["attempts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn foreign_database_open_refuses_without_changing_identity_or_journal_mode() {
    for blank in [true, false] {
        disk_fixture(|root| {
            let path = root.join("ledger.sqlite");
            let foreign = Connection::open(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            foreign.pragma_update(None, "application_id", 42).unwrap();
            if !blank {
                foreign.execute_batch("CREATE TABLE foreign_data(value TEXT); INSERT INTO foreign_data VALUES('keep'); PRAGMA journal_mode=WAL;").unwrap();
            }
            let original_mode: String = foreign
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            let original = fs::read(&path).unwrap();
            assert!(open(&path, false).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(
                foreign
                    .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                42
            );
            assert_eq!(
                foreign
                    .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                    .unwrap(),
                original_mode
            );
            if !blank {
                assert_eq!(
                    foreign
                        .query_row("SELECT value FROM foreign_data", [], |row| row
                            .get::<_, String>(0))
                        .unwrap(),
                    "keep"
                );
            }
        });
    }
}
