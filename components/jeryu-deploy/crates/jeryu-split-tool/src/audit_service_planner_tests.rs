use super::*;
use super::super::service_http;
use service::planner::Planner;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

fn configuration(fixture: &QueuedFixture) -> (PathBuf, PathBuf) {
    let route = fixture.intake.root.join("service-route.json");
    if !route.exists() { write(&route, &fixture.intake.route); }
    let planner = fixture.intake.root.join("planner.json");
    if !planner.exists() {
        write(&planner, &serde_json::to_vec(&json!({
            "schema_version":"jeryu.audit-service-planner/v1", "route_id":"jeryu-public",
            "source_repo":fixture.repo,"identity":fixture.identity,"execution_config":fixture.config,
            "governing_policy":fixture.policy,"candidate_policy":fixture.policy,
        })).unwrap());
    }
    (route, planner)
}

fn open(fixture: &QueuedFixture) -> (Arc<service::Receiver>, Planner) {
    let (route, planner) = configuration(fixture);
    let receiver = service::Receiver::open(&fixture.intake.database, &[route]).unwrap();
    let planner = Planner::open(&receiver, &[planner]).unwrap();
    (receiver, planner)
}

#[test]
fn startup_backlog_and_restart_retries_preserve_one_atomic_queue() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    let (receiver, mut planner) = open(&fixture);
    assert!(planner.tick(2000).unwrap());
    assert!(!planner.tick(2001).unwrap());
    assert_eq!(fixture.jobs(), 2);
    drop(planner);
    drop(receiver);
    fixture.push(&first, &third, "11234567-89ab-cdef-0123-456789abcdef");
    let (_, mut restarted) = open(&fixture);
    assert!(!restarted.tick(2031).unwrap());
    assert_eq!(fixture.jobs(), 2);
    assert_eq!(fixture.plans().len(), 1);
    assert_eq!(fixture.intake.status()["receptions"].as_array().unwrap().len(), 2);
}

#[test]
fn missing_history_retries_without_starving_another_event() {
    let mut fixture = QueuedFixture::new();
    let [first, second, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    fixture.push(&first, &second, "11234567-89ab-cdef-0123-456789abcdef");
    let object = fixture.repo.join(".git/objects").join(&third[..2]).join(&third[2..]);
    let retained = fixture.intake.root.join("retained-third-object");
    fs::rename(&object, &retained).unwrap();
    let (_, mut planner) = open(&fixture);
    assert!(!planner.tick(2000).unwrap());
    assert_eq!(fixture.jobs(), 0);
    assert!(planner.tick(2001).unwrap());
    assert_eq!(fixture.jobs(), 1);
    assert!(!planner.tick(2002).unwrap());
    fs::rename(&retained, &object).unwrap();
    assert!(planner.tick(2030).unwrap());
    assert_eq!(fixture.jobs(), 2);
    assert_eq!(fixture.plans().len(), 2);
    let failures: i64 = fixture.intake.connection.query_row("SELECT count(*) FROM intake_failures", [], |row| row.get(0)).unwrap();
    assert_eq!(failures, 1);
}

#[test]
fn planner_holds_inputs_until_restart_and_refuses_bad_configuration() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    let (receiver, mut planner) = open(&fixture);
    let (_, config) = configuration(&fixture);
    assert!(Planner::open(&receiver, &[config.clone(), config.clone()]).is_err());
    let original = fs::read(&fixture.config).unwrap();
    fs::write(&fixture.config, b"{}").unwrap();
    assert!(planner.tick(2000).unwrap(), "running inputs must remain held");
    assert!(Planner::open(&receiver, std::slice::from_ref(&config)).is_err());
    fs::write(&fixture.config, original).unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["unknown"] = json!(true);
    fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Planner::open(&receiver, std::slice::from_ref(&config)).is_err());
    value.as_object_mut().unwrap().remove("unknown");
    value["route_id"] = json!("unreceived-route");
    fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Planner::open(&receiver, std::slice::from_ref(&config)).is_err());
}

#[test]
fn queue_write_failures_back_off_and_unretained_failures_stop_the_worker() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    let (_, mut planner) = open(&fixture);
    fixture.intake.connection.execute_batch("CREATE TRIGGER fixture_no_link BEFORE INSERT ON intake_plan_links BEGIN SELECT RAISE(ABORT,'synthetic link fault'); END;").unwrap();
    assert!(!planner.tick(2000).unwrap());
    assert_eq!(fixture.jobs(), 0);
    assert!(fixture.plans().is_empty());
    assert!(!planner.tick(2001).unwrap());
    fixture.intake.connection.execute_batch("CREATE TRIGGER fixture_no_failure BEFORE INSERT ON intake_failures BEGIN SELECT RAISE(ABORT,'synthetic failure custody fault'); END;").unwrap();
    assert!(planner.tick(2030).is_err());
    assert_eq!(fixture.jobs(), 0);
    fixture.intake.connection.execute_batch("DROP TRIGGER fixture_no_link; DROP TRIGGER fixture_no_failure;").unwrap();
    assert!(planner.tick(2031).unwrap());
    assert_eq!(fixture.jobs(), 2);
}

async fn start(fixture: &QueuedFixture) -> (SocketAddr, oneshot::Sender<()>, JoinHandle<Result<()>>) {
    let (receiver, planner) = open(fixture);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(service::serve_with_planner(listener, receiver, Some(planner), async move {
        let _ = stopped.await;
    }));
    (address, stop, task)
}

async fn wait_for_jobs(fixture: &QueuedFixture, count: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.jobs() != count { tokio::time::sleep(Duration::from_millis(25)).await; }
    }).await.unwrap();
}

#[test]
fn real_http_intake_automatically_queues_and_survives_restart() {
    let fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    let body = push(&first, &third, "refs/heads/main");
    service_http::runtime().block_on(async {
        for reception in 1..=2 {
            let (address, stop, task) = start(&fixture).await;
            let (code, response) = service_http::request(address, body.clone(), service_http::wire_headers(&body)).await;
            assert_eq!(code, 202);
            assert_eq!(response["reception_id"], reception);
            assert_eq!(response["audit_execution_admitted"], false);
            assert_eq!(response["publication_qualified"], false);
            wait_for_jobs(&fixture, 2).await;
            stop.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap().unwrap();
        }
    });
    assert_eq!(fixture.plans().len(), 1);
    assert_eq!(fixture.intake.status()["receptions"].as_array().unwrap().len(), 2);
}

#[test]
fn fatal_planner_storage_failure_closes_the_actual_listener() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    fixture.intake.connection.execute_batch("CREATE TRIGGER fixture_no_link BEFORE INSERT ON intake_plan_links BEGIN SELECT RAISE(ABORT,'synthetic link fault'); END;
        CREATE TRIGGER fixture_no_failure BEFORE INSERT ON intake_failures BEGIN SELECT RAISE(ABORT,'synthetic failure custody fault'); END;").unwrap();
    service_http::runtime().block_on(async {
        let (address, _stop, task) = start(&fixture).await;
        let result = tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
        assert!(result.is_err());
        assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_err());
    });
    assert_eq!(fixture.jobs(), 0);
    assert!(fixture.plans().is_empty());
}

#[test]
fn blocked_git_planning_does_not_hold_up_real_http_reception() {
    let mut fixture = QueuedFixture::new();
    let [first, _, third] = fixture.commits.clone();
    fixture.push(&first, &third, GUID);
    let config = fixture.repo.join(".git/config");
    let saved = fixture.intake.root.join("saved-git-config");
    fs::rename(&config, &saved).unwrap();
    let status = std::process::Command::new("/usr/bin/mkfifo").arg(&config).status().unwrap();
    assert!(status.success());
    let (entered, entrance) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let fifo = config.clone();
    let writer = std::thread::spawn(move || {
        let held = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(held); // Release the actual Git reader with an empty valid config.
    });
    service_http::runtime().block_on(async {
        let (address, stop, task) = start(&fixture).await;
        tokio::task::spawn_blocking(move || entrance.recv_timeout(Duration::from_secs(3)).unwrap()).await.unwrap();
        let body = push(&first, &third, "refs/heads/main");
        let response = tokio::time::timeout(Duration::from_secs(2),
            service_http::request(address, body.clone(), service_http::wire_headers(&body))).await.unwrap();
        assert_eq!(response.0, 202);
        assert_eq!(fixture.jobs(), 0, "Git is still held while HTTP acknowledges durable intake");
        fs::rename(&config, fixture.intake.root.join("retained-config-fifo")).unwrap();
        fs::rename(&saved, &config).unwrap();
        release.send(()).unwrap();
        writer.join().unwrap();
        wait_for_jobs(&fixture, 2).await;
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap().unwrap();
    });
}
