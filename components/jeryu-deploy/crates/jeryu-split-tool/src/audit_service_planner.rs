//! Restartable local planning from durable receptions. No source fetch or execution.
use super::*;
use tokio::sync::watch;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    schema_version: String,
    route_id: String,
    source_repo: PathBuf,
    identity: PathBuf,
    execution_config: PathBuf,
    governing_policy: PathBuf,
    candidate_policy: PathBuf,
}

struct Binding {
    route_hash: String,
    identity_hash: String,
    source: PathBuf,
    inputs: queue::Inputs,
}

pub(in crate::audit_intake) struct Planner {
    database: Database,
    bindings: Vec<Binding>,
    next: usize,
}

impl Planner {
    pub(in crate::audit_intake) fn open(receiver: &Receiver, paths: &[PathBuf]) -> Result<Self> {
        ensure!(
            !paths.is_empty() && paths.len() <= 64,
            "one to 64 planner routes required"
        );
        let mut configured = std::collections::BTreeSet::new();
        let mut bindings = Vec::new();
        for path in paths {
            let JsonObject(config): JsonObject<Configuration> =
                serde_json::from_slice(&input::read_private(path, MAX_CONFIG)?)?;
            ensure!(
                config.schema_version == "jeryu.audit-service-planner/v1",
                "unsupported planner configuration"
            );
            ensure!(
                configured.insert(config.route_id.clone()),
                "duplicate planner route"
            );
            let route = receiver
                .routes
                .get(&config.route_id)
                .context("planner route is not received here")?;
            ensure!(
                config.source_repo.is_absolute(),
                "planner source path must be absolute"
            );
            let inputs = queue::Inputs::read(
                &config.identity,
                &config.execution_config,
                &config.governing_policy,
                &config.candidate_policy,
            )?;
            inputs.bind(&Route::parse(&route.bytes)?)?;
            bindings.push(Binding {
                route_hash: audit_evidence::hash(&route.bytes),
                identity_hash: inputs.identity_hash()?,
                source: config.source_repo,
                inputs,
            });
        }
        let received = receiver
            .database
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver database unavailable"))?;
        received.check_name()?;
        let database = Database {
            path: received.path.clone(),
            identity: received.identity,
            connection: audit_ledger::open(&received.path, false)?,
        };
        database.check_name()?;
        Ok(Self {
            database,
            bindings,
            next: 0,
        })
    }

    /// At most one due event; rotate routes so a broken source cannot monopolize work.
    pub(in crate::audit_intake) fn tick(&mut self, now: i64) -> Result<bool> {
        ensure!(now >= 0, "invalid planner time");
        self.database.check_name()?;
        let binding = &self.bindings[self.next];
        self.next = (self.next + 1) % self.bindings.len();
        let cutoff = now.saturating_sub(30);
        let pending: Option<(String, i64)> = self.database.connection.query_row(
            "SELECT e.key,MIN(r.id) FROM intake_events e
             JOIN intake_classifications c ON c.event_key=e.key
             JOIN intake_receptions r ON r.id=c.reception_id
             WHERE r.route_sha256=?1 AND r.authentication='matched'
               AND json_extract(c.metadata,'$.reception_accepted')=1
               AND json_extract(c.metadata,'$.translation.state')='pending_plan'
               AND NOT EXISTS(SELECT 1 FROM intake_plan_links l WHERE l.event_key=e.key AND l.identity_sha256=?2)
               AND NOT EXISTS(SELECT 1 FROM intake_failures f WHERE f.event_key=e.key AND f.kind='planner_error' AND f.recorded_at>?3)
             GROUP BY e.key
             ORDER BY COALESCE((SELECT MAX(f.recorded_at) FROM intake_failures f WHERE f.event_key=e.key AND f.kind='planner_error'),0),MIN(r.id)
             LIMIT 1",
            params![binding.route_hash,binding.identity_hash,cutoff],
            |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let Some((event, reception)) = pending else {
            return Ok(false);
        };
        let latest_failure = |connection: &Connection| -> Result<i64> {
            Ok(connection.query_row(
                "SELECT COALESCE(MAX(id),0) FROM intake_failures WHERE event_key=?1",
                [&event],
                |row| row.get(0),
            )?)
        };
        let before = latest_failure(&self.database.connection)?;
        let result = queue::run(
            &mut self.database.connection,
            &event,
            reception,
            &binding.source,
            Ok(binding.inputs.clone()),
            now,
        );
        self.database.check_name()?;
        match result {
            Ok(_) => Ok(true),
            Err(error) => {
                ensure!(
                    latest_failure(&self.database.connection)? > before,
                    "planner failed without durable failure custody: {error:#}"
                );
                // Exact diagnostics remain private in the append-only database.
                eprintln!("audit planner retained pending failure for event {event}");
                Ok(false)
            }
        }
    }

    pub(in crate::audit_intake) fn run(mut self, stop: watch::Receiver<bool>) -> Result<()> {
        loop {
            if *stop.borrow() || stop.has_changed().is_err() {
                return Ok(());
            }
            self.tick(audit_ledger::now()?)?;
            // A separate blocking worker owns Git and its own SQLite connection.
            // No HTTP request waits for a graph read or retry delay.
            for _ in 0..20 {
                if *stop.borrow() || stop.has_changed().is_err() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
