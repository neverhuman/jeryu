//! Small process-level probes for the ignored real OCI qualification test.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::symlink,
    process::{Child, Command},
    time::Duration,
};

fn emit(value: serde_json::Value) {
    println!("{value}");
    io::stdout().flush().expect("flush probe evidence");
}

fn limit(name: &str) -> String {
    fs::read_to_string(format!("/sys/fs/cgroup/{name}"))
        .expect("required cgroup v2 file")
        .trim()
        .to_owned()
}

fn event(name: &str, key: &str) -> u64 {
    limit(name)
        .lines()
        .find_map(|line| {
            let (name, count) = line.split_once(' ')?;
            (name == key).then(|| count.parse().expect("numeric cgroup event"))
        })
        .expect("required cgroup event")
}

struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str).expect("probe mode") {
        "identity" => emit(
            json!({"source_sha256": Sha256::digest(include_bytes!("oci_probe.rs"))
            .iter()
            .fold(String::with_capacity(64), |mut out, byte| {
                out.push_str(&format!("{byte:02x}"));
                out
            })}),
        ),
        "idle" => std::thread::sleep(Duration::from_secs(60)),
        "filesystem" => {
            fs::write("/workspace/write-proof", b"workspace works").expect("workspace write");
            fs::write("/tmp/write-proof", b"tmpfs works").expect("tmpfs write");
            let outside = fs::write("/oci-root-proof/write-proof", b"root write");
            emit(json!({"probe":"filesystem","workspace":true,"tmpfs":true,
                "outside_written":outside.is_ok(),"outside_errno":outside.err().and_then(|e| e.raw_os_error())}));
        }
        "sockets" => {
            let paths = [
                "/var/run/docker.sock",
                "/run/docker.sock",
                "/run/podman/podman.sock",
                "/var/run/podman/podman.sock",
            ];
            let absent = paths.iter().all(|path| match fs::symlink_metadata(path) {
                Err(error) => error.kind() == io::ErrorKind::NotFound,
                Ok(_) => false,
            });
            emit(json!({"probe":"sockets","absent":absent}));
        }
        "environment" => {
            let keys = [
                "SSH_AUTH_SOCK",
                "DOCKER_HOST",
                "AWS_ACCESS_KEY_ID",
                "AWS_SECRET_ACCESS_KEY",
                "AWS_SESSION_TOKEN",
                "GOOGLE_APPLICATION_CREDENTIALS",
                "AZURE_CLIENT_SECRET",
                "GITHUB_TOKEN",
            ];
            emit(
                json!({"probe":"environment","absent":keys.iter().all(|key| std::env::var_os(key).is_none())}),
            );
        }
        "network" => {
            let address: SocketAddr = args
                .get(2)
                .expect("listener address")
                .parse()
                .expect("socket address");
            let result = TcpStream::connect_timeout(&address, Duration::from_secs(3));
            emit(json!({"probe":"network","connected":result.is_ok(),
                "errno":result.err().and_then(|error| error.raw_os_error())}));
        }
        "pids" => {
            let before = event("pids.events", "max");
            let mut children = Children(Vec::new());
            let mut error = None;
            for _ in 0..40 {
                match Command::new(std::env::current_exe().expect("probe executable"))
                    .arg("idle")
                    .spawn()
                {
                    Ok(child) => children.0.push(child),
                    Err(failure) => {
                        error = failure.raw_os_error();
                        break;
                    }
                }
            }
            emit(
                json!({"probe":"pids","spawned":children.0.len(),"errno":error,
                "max":limit("pids.max"),"current":limit("pids.current"),
                "max_events_delta":event("pids.events","max") - before}),
            );
        }
        "memory" => {
            emit(json!({"probe":"memory","phase":"started","max":limit("memory.max")}));
            let bytes = vec![0x5a_u8; 96 * 1024 * 1024];
            std::hint::black_box(&bytes);
            emit(json!({"probe":"memory","phase":"finished","bytes":bytes.len()}));
        }
        "syscalls" => {
            let link = symlink("/does-not-exist", "/workspace/seccomp-proof-link");
            let result = Command::new("/usr/bin/unshare")
                .args(["--", "/usr/bin/true"])
                .env("LC_ALL", "C")
                .output()
                .expect("verified image unshare command");
            emit(json!({"probe":"syscalls","symlink_ok":link.is_ok(),
                "symlink_errno":link.err().and_then(|error| error.raw_os_error()),
                "unshare_ok":result.status.success(),"unshare_exit":result.status.code(),
                "unshare_eperm":String::from_utf8_lossy(&result.stderr).contains("Operation not permitted")}));
        }
        "status" => {
            let status = fs::read_to_string("/proc/self/status").expect("process status");
            let field = |name: &str| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix(name))
                    .expect("required process status field")
                    .trim()
                    .to_owned()
            };
            emit(
                json!({"probe":"status","uid":field("Uid:"),"gid":field("Gid:"),
                "no_new_privs":field("NoNewPrivs:"),"cap_eff":field("CapEff:"),
                "cap_prm":field("CapPrm:"),"cap_bnd":field("CapBnd:"),"seccomp":field("Seccomp:")}),
            );
        }
        "cgroups" => {
            emit(json!({"probe":"cgroups","memory":limit("memory.max"),"pids":limit("pids.max")}))
        }
        other => panic!("unknown probe: {other}"),
    }
}
