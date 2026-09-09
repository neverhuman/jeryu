//! Required real Docker proof. Ordinary Cargo runs explicitly ignore this test.
//! The root OCI lane builds oci_probe and runs this exact test with --include-ignored.
use anyhow::{Context, Result, bail, ensure};
use jeryu_runner_core::{
    job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy},
    policy::select_runner,
    sandbox::SandboxPlan,
    trust::{RunnerClass, TrustTier},
};
use jeryu_runner_oci::OciSpec;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs::{self, OpenOptions},
    io::Write,
    net::TcpListener,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const BASE: &str = "docker.io/library/ubuntu@sha256:1e0a86e57d247923571b75e0aaf48a1449cf8c543d51fb3e07a4a7d7bfa79316";
const BASE_ID: &str = "sha256:a6f81fb630d51837271b89f8193810a5fc493fa4f30a55d7ebcdb3a66f3cc63a";
const DOCKER: &str = "/usr/bin/docker";
const MIB: u64 = 1024 * 1024;

struct Captured {
    code: Option<i32>,
    out: String,
    err: String,
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn physical(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "path must be absolute");
    ensure!(
        fs::canonicalize(path)? == path,
        "path must not traverse symlinks"
    );
    Ok(path.to_owned())
}

fn validate_container_status(code: Option<i32>, state: &Value, expected: i32) -> Result<()> {
    ensure!(
        !matches!(code, Some(125..=127)),
        "engine/entrypoint error is not a sandbox verdict"
    );
    ensure!(code == Some(expected), "unexpected Docker attachment exit");
    ensure!(
        state["Status"] == "exited" && state["ExitCode"] == expected,
        "container did not exit as expected"
    );
    ensure!(
        state["Error"] == "",
        "Docker reported a container execution error"
    );
    ensure!(
        state["OOMKilled"].as_bool() == Some(expected == 137),
        "missing or contradictory OOM evidence"
    );
    let started = state["StartedAt"].as_str().context("missing start time")?;
    ensure!(
        !started.is_empty() && !started.starts_with("0001-"),
        "container never started"
    );
    Ok(())
}

fn records(out: &str, mode: &str) -> Result<Vec<Value>> {
    let result: Vec<Value> = out
        .lines()
        .filter(|line| !line.is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(!result.is_empty(), "missing probe evidence");
    ensure!(
        result
            .iter()
            .all(|value| value.is_object() && value["probe"] == mode),
        "malformed or misrouted probe evidence"
    );
    Ok(result)
}

fn validate_base(base: &Value) -> Result<()> {
    let manifest = BASE.split_once('@').expect("pinned base").1;
    ensure!(
        base["Os"] == "linux" && base["Architecture"] == "amd64",
        "wrong base platform"
    );
    ensure!(
        base["RepoDigests"]
            .as_array()
            .is_some_and(|digests| digests.iter().any(|digest| {
                ["ubuntu", "library/ubuntu", "docker.io/library/ubuntu"]
                    .iter()
                    .any(|name| digest.as_str() == Some(format!("{name}@{manifest}").as_str()))
            })),
        "base is not bound to the pinned repository digest"
    );
    if base["Id"] == BASE_ID {
        return Ok(());
    }
    // The containerd image store reports the pinned manifest as Id. Only this
    // exact single-platform manifest is accepted, never an arbitrary index.
    ensure!(
        base["Id"] == manifest
            && base["Descriptor"]["digest"] == manifest
            && matches!(
                base["Descriptor"]["mediaType"].as_str(),
                Some(
                    "application/vnd.oci.image.manifest.v1+json"
                        | "application/vnd.docker.distribution.manifest.v2+json"
                )
            ),
        "immutable base identity mismatch"
    );
    Ok(())
}

struct Engine {
    scratch: TempDir,
    scratch_identity: (u64, u64),
    scratch_removed: bool,
    sequence: Cell<u64>,
    containers: Vec<String>,
    image: Option<String>,
    base: Value,
    evidence: Vec<Value>,
}

impl Engine {
    fn new() -> Result<Self> {
        let mut scratch = tempfile::Builder::new()
            .prefix("jeryu-oci-proof-")
            .tempdir_in("/tmp")?;
        // Cleanup must first inspect links and mounts, including when a probe fails.
        scratch.disable_cleanup(true);
        fs::set_permissions(scratch.path(), fs::Permissions::from_mode(0o700))?;
        fs::create_dir(scratch.path().join("docker-config"))?;
        let metadata = fs::symlink_metadata(scratch.path())?;
        Ok(Self {
            scratch,
            scratch_identity: (metadata.dev(), metadata.ino()),
            scratch_removed: false,
            sequence: Cell::new(0),
            containers: Vec::new(),
            image: None,
            base: Value::Null,
            evidence: Vec::new(),
        })
    }

    fn capture(&self, command: &mut Command, seconds: u64) -> Result<Captured> {
        let serial = self.sequence.get();
        self.sequence.set(serial + 1);
        let stdout = self.scratch.path().join(format!("command-{serial}.stdout"));
        let stderr = self.scratch.path().join(format!("command-{serial}.stderr"));
        let file = |path: &Path| OpenOptions::new().create_new(true).write(true).open(path);
        command.process_group(0);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(file(&stdout)?)
            .stderr(file(&stderr)?)
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(seconds);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", "--"])
                    .arg(format!("-{}", child.id()))
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                bail!("command exceeded {seconds}s deadline");
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        ensure!(
            fs::metadata(&stdout)?.len() <= 4 * MIB && fs::metadata(&stderr)?.len() <= 4 * MIB,
            "oversized command output"
        );
        Ok(Captured {
            code: status.code(),
            out: fs::read_to_string(stdout)?,
            err: fs::read_to_string(stderr)?,
        })
    }

    fn docker(&self, args: &[String], seconds: u64) -> Result<Captured> {
        let mut command = Command::new(DOCKER);
        command
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("HOME", self.scratch.path())
            .env("LC_ALL", "C")
            .args(["--host", "unix:///run/docker.sock", "--config"])
            .arg(self.scratch.path().join("docker-config"))
            .args(args);
        // Deliberate non-secret ambient canaries: none may enter the container.
        for key in [
            "SSH_AUTH_SOCK",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "AZURE_CLIENT_SECRET",
            "GITHUB_TOKEN",
        ] {
            command.env(key, "oci-proof-canary");
        }
        self.capture(&mut command, seconds)
    }

    fn successful(&self, args: &[String], seconds: u64) -> Result<String> {
        let output = self.docker(args, seconds)?;
        ensure!(
            output.code == Some(0),
            "required Docker operation failed: {}",
            output.err
        );
        Ok(output.out)
    }

    fn json(&self, args: &[String]) -> Result<Value> {
        Ok(serde_json::from_str(&self.successful(args, 30)?)?)
    }

    fn build_image(&mut self, probe: &Path) -> Result<()> {
        self.successful(&strings(&["pull", "--platform", "linux/amd64", BASE]), 180)?;
        let base = self.json(&strings(&["image", "inspect", BASE]))?;
        validate_base(&base[0])?;
        self.base = base[0].clone();
        let context = self.scratch.path().join("image-context");
        fs::create_dir(&context)?;
        fs::copy(probe, context.join("oci-probe"))?;
        fs::set_permissions(context.join("oci-probe"), fs::Permissions::from_mode(0o555))?;
        fs::write(
            context.join("Dockerfile"),
            format!(
                "FROM {BASE}\nCOPY oci-probe /usr/local/bin/oci-probe\nRUN mkdir /oci-root-proof && chmod 1777 /oci-root-proof\n"
            ),
        )?;
        let iid = self.scratch.path().join("image.id");
        self.successful(
            &[
                "build".into(),
                "--network=none".into(),
                "--pull=false".into(),
                "--no-cache".into(),
                "--iidfile".into(),
                iid.display().to_string(),
                context.display().to_string(),
            ],
            180,
        )?;
        let image = fs::read_to_string(iid)?.trim().to_owned();
        ensure!(
            image
                .strip_prefix("sha256:")
                .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())),
            "build did not return a content-addressed image"
        );
        self.image = Some(image);
        Ok(())
    }

    fn run(&mut self, spec: &OciSpec, mode: &str, expected: i32) -> Result<Vec<Value>> {
        self.run_args(spec, mode, expected, spec.args())
    }

    fn run_args(
        &mut self,
        spec: &OciSpec,
        mode: &str,
        expected: i32,
        mut args: Vec<String>,
    ) -> Result<Vec<Value>> {
        // Only lifecycle differs from the product argv, so Docker state survives
        // long enough to distinguish actual OOM/denial from engine startup errors.
        let name = format!(
            "{}-{}",
            self.scratch.path().file_name().unwrap().to_string_lossy(),
            self.containers.len()
        );
        self.containers.push(name.clone());
        ensure!(
            args.first().is_some_and(|arg| arg == "run")
                && args.get(1).is_some_and(|arg| arg == "--rm"),
            "OciSpec lifecycle changed; review qualification adapter"
        );
        args.splice(0..2, ["create".into(), "--name".into(), name.clone()]);
        let id = self.successful(&args, 30)?.trim().to_owned();
        ensure!(
            id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "missing container identity"
        );
        let before = self.json(&strings(&["inspect", &id]))?;
        let expected_hardening = spec.hardening.as_ref().context("agent hardening missing")?;
        let host = &before[0]["HostConfig"];
        ensure!(
            host["ReadonlyRootfs"] == args.iter().any(|arg| arg == "--read-only")
                && host["Privileged"] == false
                && host["CapDrop"]
                    .as_array()
                    .is_some_and(|caps| caps.len() == 1 && caps[0] == "ALL")
                && host["Tmpfs"]["/tmp"].as_str().is_some_and(|options| [
                    "rw", "nosuid", "nodev", "noexec"
                ]
                .iter()
                .all(|option| options.split(',').any(|actual| actual == *option))),
            "engine filesystem/capability configuration differs from OciSpec"
        );
        let security = host["SecurityOpt"]
            .as_array()
            .context("missing container security options")?;
        ensure!(
            security.iter().any(|option| option == "no-new-privileges")
                && security.iter().any(|option| option
                    .as_str()
                    .is_some_and(|value| value.starts_with("seccomp="))),
            "engine privilege/seccomp options missing"
        );
        ensure!(
            host["Memory"] == expected_hardening.memory_max_bytes
                && host["PidsLimit"] == expected_hardening.pids_max
                && host["CpuShares"] == expected_hardening.cpu_shares
                && host["NetworkMode"] == spec.network,
            "engine resource/network configuration differs from OciSpec"
        );
        ensure!(
            before[0]["Config"]["Image"] == spec.image
                && before[0]["Config"]["User"] == "1000:1000",
            "engine image/user differs from OciSpec"
        );
        let mounts = before[0]["Mounts"].as_array().context("missing mounts")?;
        ensure!(
            mounts
                .iter()
                .filter(|mount| mount["Type"] == "bind")
                .count()
                == 1
                && mounts.iter().all(|mount| {
                    (mount["Type"] == "bind"
                        && mount["Source"] == spec.workspace
                        && mount["Destination"] == "/workspace"
                        && mount["RW"] == true)
                        || (mount["Type"] == "tmpfs" && mount["Destination"] == "/tmp")
                }),
            "unexpected host mount"
        );
        let output = self.docker(&strings(&["start", "--attach", &id]), 45)?;
        let after = self.json(&strings(&["inspect", &id]))?;
        validate_container_status(output.code, &after[0]["State"], expected)?;
        if expected == 137 {
            ensure!(
                after[0]["State"]["OOMKilled"] == true,
                "SIGKILL without Docker OOM evidence"
            );
        } else {
            ensure!(after[0]["State"]["OOMKilled"] == false, "unexpected OOM");
        }
        let results = records(&output.out, mode)?;
        self.evidence
            .push(json!({"mode":mode,"argv":args,"container_id":id,
            "host_config":host,"state":after[0]["State"],"probe":results}));
        Ok(results)
    }

    fn cleanup(&mut self) -> Result<()> {
        if self.scratch_removed {
            return Ok(());
        }
        // Attempt every owned container even when a previous create/start failed.
        let mut failed = Vec::new();
        for name in std::mem::take(&mut self.containers) {
            if self
                .successful(&strings(&["rm", "--force", &name]), 10)
                .is_ok()
            {
                continue;
            }
            let filter = format!("name=^/{name}$");
            let absent = self
                .successful(
                    &strings(&["ps", "--all", "--filter", &filter, "--format", "{{.ID}}"]),
                    10,
                )
                .is_ok_and(|ids| ids.trim().is_empty());
            if !absent {
                failed.push(name);
            }
        }
        self.containers = failed;
        let mut image_removed = true;
        if let Some(image) = &self.image {
            if self
                .successful(&strings(&["image", "rm", image]), 20)
                .is_ok()
            {
                self.image = None;
            } else {
                image_removed = false;
            }
        }
        ensure!(
            self.containers.is_empty() && image_removed,
            "could not remove every owned OCI resource"
        );
        let links = inspect_scratch(self.scratch.path(), self.scratch_identity)?;
        eprintln!("OCI cleanup inspected {links} symlinks without following their targets");
        fs::remove_dir_all(self.scratch.path())?;
        self.scratch_removed = true;
        Ok(())
    }
}

fn inspect_scratch(root: &Path, identity: (u64, u64)) -> Result<usize> {
    physical(root)?;
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && (metadata.dev(), metadata.ino()) == identity,
        "scratch root was replaced; preserve it for inspection"
    );
    for line in fs::read_to_string("/proc/self/mountinfo")?.lines() {
        let path = line
            .split_whitespace()
            .nth(4)
            .context("malformed mount table")?;
        let path = path
            .replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\012", "\n")
            .replace("\\134", "\\");
        ensure!(
            !Path::new(&path).starts_with(root),
            "scratch contains a mount; refuse deletion"
        );
    }
    fn inspect(path: &Path, device: u64) -> Result<usize> {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(metadata.dev() == device, "scratch crosses a filesystem");
        if metadata.file_type().is_symlink() {
            // Inspect the link itself; never traverse the target during deletion.
            fs::read_link(path)?;
            return Ok(1);
        }
        if metadata.is_file() {
            return Ok(0);
        }
        ensure!(metadata.is_dir(), "scratch contains a special node");
        fs::read_dir(path)?.try_fold(0, |count, entry| {
            Ok(count + inspect(&entry?.path(), device)?)
        })
    }
    inspect(root, identity.0)
}

impl Drop for Engine {
    fn drop(&mut self) {
        if self.cleanup().is_err() {
            eprintln!("OCI proof cleanup failed; discard this disposable VM");
        }
    }
}

fn spec(
    workspace: &Path,
    image: &str,
    profile: &Path,
    memory: u64,
    pids: u32,
    mode: &str,
) -> Result<OciSpec> {
    let job = JobRequest {
        job_id: "oci-qualification".into(),
        repo_id: "jeryu/jeryu".into(),
        commit_sha: "proof".into(),
        workspace: workspace.to_owned(),
        command: "/usr/local/bin/oci-probe".into(),
        args: vec![mode.into()],
        env: Default::default(),
        trust_tier: TrustTier::T4ForkPr,
        requested_runner: Some(RunnerClass::OciDocker),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::None,
        timeout_ms: 45_000,
        fork: true,
    };
    let decision = select_runner(&job).map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut plan = SandboxPlan::from_decision(workspace, &decision);
    ensure!(
        plan.cgroup_limits.memory_max_bytes == 4 * 1024 * MIB
            && plan.cgroup_limits.pids_max == 1024
            && plan.cgroup_limits.cpu_weight == 100,
        "default OCI resource profile changed; review qualification"
    );
    let expected_profile = format!("{}.json", plan.seccomp.name);
    ensure!(
        profile.file_name().and_then(|name| name.to_str()) == Some(expected_profile.as_str()),
        "owned seccomp profile does not match the runtime plan"
    );
    plan.cgroup_limits.memory_max_bytes = memory;
    plan.cgroup_limits.pids_max = pids;
    let mut spec =
        OciSpec::from_agent_job(&job, &plan).map_err(|error| anyhow::anyhow!("{error}"))?;
    spec.runtime = DOCKER.into();
    spec.image = image.into();
    spec.hardening
        .as_mut()
        .context("agent hardening missing")?
        .seccomp_profile_path = profile.display().to_string();
    ensure!(spec.network == "none", "network-deny agent profile changed");
    Ok(spec)
}

fn source_identity(engine: &Engine, component: &Path) -> Result<Value> {
    let root = if component
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "components")
    {
        component
            .parent()
            .unwrap()
            .parent()
            .context("monorepo root")?
    } else {
        component
    };
    physical(root)?;
    ensure!(
        fs::symlink_metadata(root.join(".git"))?.is_dir(),
        "ordinary source checkout required"
    );
    let git = |args: &[&str]| -> Result<String> {
        let mut command = Command::new("/usr/bin/git");
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            // The root-only test may run against an unprivileged checkout.
            // Trust only this verified physical root, without ambient Git config.
            .args(["-c", "core.fsmonitor=false", "-c"])
            .arg(format!("safe.directory={}", root.display()))
            .arg("-C")
            .arg(component)
            .args(args);
        let result = engine.capture(&mut command, 10)?;
        ensure!(result.code == Some(0), "source identity command failed");
        Ok(result.out.trim().to_owned())
    };
    ensure!(
        git(&["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty(),
        "OCI qualification requires clean committed source"
    );
    Ok(json!({"commit":git(&["rev-parse","HEAD"])?, "tree":git(&["rev-parse","HEAD^{tree}"])?}))
}

#[test]
#[ignore = "requires disposable Linux x86_64 Docker host; run through the root OCI lane"]
fn hardened_oci_profile_enforces_required_matrix() -> Result<()> {
    ensure!(
        std::env::var("JERYU_DISPOSABLE_SANDBOX").as_deref() == Ok("1"),
        "disposable VM acknowledgement required"
    );
    ensure!(
        fs::metadata("/proc/self")?.uid() == 0,
        "root in the disposable VM is required"
    );
    ensure!(
        std::env::consts::OS == "linux" && std::env::consts::ARCH == "x86_64",
        "Linux x86_64 required"
    );
    let socket = fs::symlink_metadata("/run/docker.sock")?;
    ensure!(
        socket.file_type().is_socket() && socket.uid() == 0 && socket.mode() & 0o002 == 0,
        "root-owned local Docker socket required"
    );
    physical(Path::new(DOCKER))?;
    let probe = physical(Path::new(
        &std::env::var("JERYU_OCI_PROBE_BIN")
            .context("root lane must build and supply oci_probe")?,
    ))?;
    let output = physical(Path::new(
        &std::env::var("JERYU_OCI_OUTPUT_DIR")
            .context("root lane must create a private empty evidence directory")?,
    ))?;
    ensure!(
        fs::read_dir(&output)?.next().is_none()
            && fs::metadata(&output)?.permissions().mode() & 0o077 == 0
            && fs::metadata(&output)?.uid() == 0,
        "evidence directory must be private and empty"
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let profile =
        physical(&component.join("images/agent-sandbox/seccomp/oci-docker-phase4-seccomp.json"))?;
    let profile_digest = digest(&profile)?;
    let probe_digest = digest(&probe)?;
    let mut engine = Engine::new()?;
    let source = source_identity(&engine, component)?;
    let identity = engine.capture(Command::new(&probe).arg("identity"), 10)?;
    ensure!(identity.code == Some(0), "probe identity process failed");
    let identity: Value = serde_json::from_str(identity.out.trim())?;
    ensure!(
        identity["source_sha256"]
            == format!(
                "{:x}",
                Sha256::digest(include_bytes!("../examples/oci_probe.rs"))
            ),
        "probe was built from stale source"
    );
    let info = engine.json(&strings(&["info", "--format", "{{json .}}"]))?;
    ensure!(
        info["OSType"] == "linux" && info["CgroupVersion"] == "2",
        "Linux cgroup v2 Docker daemon required"
    );
    let security = info["SecurityOptions"]
        .as_array()
        .context("missing daemon security options")?;
    ensure!(
        security
            .iter()
            .any(|entry| entry.as_str().is_some_and(|v| v.contains("name=seccomp")))
            && !security
                .iter()
                .any(|entry| entry.as_str().is_some_and(|v| v.contains("rootless"))),
        "rootful daemon with seccomp required"
    );
    let controllers = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers")?;
    ensure!(
        ["memory", "pids", "cpu"]
            .iter()
            .all(|controller| controllers
                .split_whitespace()
                .any(|actual| actual == *controller)),
        "required kernel cgroup controllers unavailable"
    );
    engine.build_image(&probe)?;
    let image = engine.image.clone().unwrap();
    let workspace = engine.scratch.path().join("workspace");
    fs::create_dir(&workspace)?;
    // Only this fresh private child is writable by the container's UID.
    std::os::unix::fs::chown(&workspace, Some(1000), Some(1000))?;
    fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700))?;
    let make =
        |mode: &str, memory: u64, pids: u32| spec(&workspace, &image, &profile, memory, pids, mode);

    let filesystem = make("filesystem", 64 * MIB, 64)?;
    let value = engine.run(&filesystem, "filesystem", 0)?.pop().unwrap();
    ensure!(
        value["workspace"] == true
            && value["tmpfs"] == true
            && value["outside_errno"] == 30
            && value["outside_written"] == false,
        "read-only/workspace proof failed"
    );
    let mut writable = filesystem.args();
    writable.retain(|arg| arg != "--read-only");
    // The positive control keeps every other OciSpec flag. Run through the same
    // lifecycle adapter, changing only the read-only argument.
    let readonly_control = engine
        .run_args(&filesystem, "filesystem", 0, writable)?
        .pop()
        .unwrap();
    ensure!(
        readonly_control["outside_written"] == true,
        "root write positive control failed"
    );

    ensure!(
        engine
            .run(&make("sockets", 64 * MIB, 64)?, "sockets", 0)?
            .pop()
            .unwrap()["absent"]
            == true,
        "host socket exposure"
    );
    ensure!(
        engine
            .run(&make("environment", 64 * MIB, 64)?, "environment", 0)?
            .pop()
            .unwrap()["absent"]
            == true,
        "ambient credential exposure"
    );

    let mut environment_control = make("environment", 64 * MIB, 64)?;
    environment_control
        .env
        .push(("AWS_ACCESS_KEY_ID".into(), "oci-proof-canary".into()));
    ensure!(
        engine
            .run(&environment_control, "environment", 0)?
            .pop()
            .unwrap()["absent"]
            == false,
        "environment detector positive control failed"
    );

    let listener = TcpListener::bind(("0.0.0.0", 0))?;
    let bridge = engine.json(&strings(&["network", "inspect", "bridge"]))?;
    let gateway = bridge[0]["IPAM"]["Config"][0]["Gateway"]
        .as_str()
        .context("bridge gateway unavailable")?;
    let address = format!("{gateway}:{}", listener.local_addr()?.port());
    let mut network = make("network", 64 * MIB, 64)?;
    network.command.push(address);
    let denied = engine.run(&network, "network", 0)?.pop().unwrap();
    ensure!(
        denied["connected"] == false && denied["errno"] == 101,
        "network-none did not prove ENETUNREACH"
    );
    network.network = "bridge".into();
    ensure!(
        engine.run(&network, "network", 0)?.pop().unwrap()["connected"] == true,
        "local listener positive control failed"
    );

    let pid_result = engine
        .run(&make("pids", 64 * MIB, 32)?, "pids", 0)?
        .pop()
        .unwrap();
    ensure!(
        pid_result["max"] == "32"
            && pid_result["errno"] == 11
            && pid_result["max_events_delta"]
                .as_u64()
                .is_some_and(|n| n > 0)
            && pid_result["current"]
                .as_str()
                .context("missing pids.current")?
                .parse::<u64>()?
                <= 32,
        "PID-limit proof lacks EAGAIN and cgroup evidence"
    );
    let pid_control = engine
        .run(&make("pids", 64 * MIB, 128)?, "pids", 0)?
        .pop()
        .unwrap();
    ensure!(
        pid_control["spawned"] == 40 && pid_control["errno"].is_null(),
        "PID positive control failed"
    );

    let memory = engine.run(&make("memory", 32 * MIB, 64)?, "memory", 137)?;
    ensure!(
        memory.len() == 1
            && memory[0]["phase"] == "started"
            && memory[0]["max"]
                .as_str()
                .and_then(|v| v.parse::<u64>().ok())
                == Some(32 * MIB),
        "OOM lacks a live bounded-allocation start receipt"
    );
    let memory_control = engine.run(&make("memory", 256 * MIB, 64)?, "memory", 0)?;
    ensure!(
        memory_control.len() == 2
            && memory_control[1]["phase"] == "finished"
            && memory_control[1]["bytes"] == 96 * MIB,
        "memory allocation positive control failed"
    );

    let syscalls = make("syscalls", 64 * MIB, 64)?;
    let denied = engine.run(&syscalls, "syscalls", 0)?.pop().unwrap();
    ensure!(
        denied["symlink_errno"] == 1
            && denied["symlink_ok"] == false
            && denied["unshare_exit"] == 1
            && denied["unshare_eperm"] == true,
        "owned seccomp denial failed"
    );
    let mut unfiltered = syscalls;
    unfiltered.hardening.as_mut().unwrap().seccomp_profile_path = "unconfined".into();
    let allowed = engine.run(&unfiltered, "syscalls", 0)?.pop().unwrap();
    ensure!(
        allowed["symlink_ok"] == true && allowed["unshare_ok"] == true,
        "syscall positive control failed"
    );

    let status = engine
        .run(&make("status", 64 * MIB, 64)?, "status", 0)?
        .pop()
        .unwrap();
    for field in ["uid", "gid"] {
        let ids: Vec<_> = status[field]
            .as_str()
            .context("missing identity")?
            .split_whitespace()
            .collect();
        ensure!(
            ids.len() == 4 && ids.iter().all(|value| *value == "1000"),
            "container is not consistently UID/GID 1000"
        );
    }
    for field in ["cap_eff", "cap_prm", "cap_bnd"] {
        ensure!(
            u64::from_str_radix(
                status[field].as_str().context("missing capability mask")?,
                16
            )? == 0,
            "capabilities were not dropped"
        );
    }
    ensure!(
        status["no_new_privs"] == "1" && status["seccomp"] == "2",
        "privilege/seccomp admission failed"
    );
    let limits = engine
        .run(&make("cgroups", 64 * MIB, 64)?, "cgroups", 0)?
        .pop()
        .unwrap();
    ensure!(
        limits["memory"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok())
            == Some(64 * MIB)
            && limits["pids"] == "64",
        "cgroup limit values differ"
    );

    ensure!(
        digest(&profile)? == profile_digest
            && digest(&probe)? == probe_digest
            && source_identity(&engine, component)? == source,
        "proof inputs changed"
    );
    let evidence = json!({"schema_version":"jeryu.oci-proof/v1","profile":"from_agent_job/network-deny",
        "source":source,"kernel":fs::read_to_string("/proc/sys/kernel/osrelease")?.trim(),"base":BASE,
        "expected_base_config_id":BASE_ID,"base_inspect":engine.base,"probe_image_id":image,
        "probe_sha256":probe_digest,"seccomp_sha256":profile_digest,
        "checks":["workspace-root","host-sockets","credential-env","network-egress","pids","memory",
            "seccomp","no-new-privileges","cgroup-limits"],"failures":0,"skipped":0,
        "daemon":{"version":info["ServerVersion"],"cgroup_version":info["CgroupVersion"],
            "security_options":security},"runs":engine.evidence});
    engine
        .cleanup()
        .context("required container/image cleanup")?;
    physical(&output)?;
    let temporary = output.join(".receipt.tmp");
    let mut receipt = OpenOptions::new()
        .create_new(true)
        .mode(0o600)
        .write(true)
        .open(&temporary)?;
    receipt.write_all(serde_json::to_string_pretty(&evidence)?.as_bytes())?;
    receipt.write_all(b"\n")?;
    receipt.sync_all()?;
    drop(receipt);
    fs::hard_link(&temporary, output.join("receipt.json"))?;
    fs::remove_file(temporary)?;
    fs::File::open(&output)?.sync_all()?;
    println!("OCI proof: nine checks passed, zero skipped; receipt.json written after cleanup");
    Ok(())
}

#[test]
fn engine_failures_never_count_as_sandbox_success() {
    let state =
        json!({"Status":"exited","ExitCode":125,"Error":"","StartedAt":"2026-09-09T00:00:00Z"});
    assert!(validate_container_status(Some(125), &state, 125).is_err());
    assert!(validate_container_status(Some(127), &state, 127).is_err());
    assert!(validate_container_status(None, &state, 0).is_err());
    assert!(records("{}", "memory").is_err());
    assert!(records("{\"probe\":\"sockets\"}", "memory").is_err());
    assert!(records("", "memory").is_err());
    assert!(records("[]", "memory").is_err());
    let mut success = json!({"Status":"exited","ExitCode":0,"Error":"","StartedAt":"2026-09-09T00:00:00Z","OOMKilled":false});
    assert!(validate_container_status(Some(0), &success, 0).is_ok());
    success["ExitCode"] = json!(137);
    assert!(validate_container_status(Some(137), &success, 137).is_err());
    success["OOMKilled"] = json!(true);
    assert!(validate_container_status(Some(137), &success, 137).is_ok());
}

#[test]
fn scratch_cleanup_refuses_replacement_and_does_not_follow_links() -> Result<()> {
    let mut engine = Engine::new()?;
    let mut outside = Engine::new()?;
    let canary = outside.scratch.path().join("canary");
    fs::write(&canary, b"retained")?;
    std::os::unix::fs::symlink(
        outside.scratch.path(),
        engine.scratch.path().join("outside"),
    )?;
    ensure!(inspect_scratch(engine.scratch.path(), engine.scratch_identity)? == 1);
    let held = engine.scratch.path().with_extension("held");
    fs::rename(engine.scratch.path(), &held)?;
    fs::create_dir(engine.scratch.path())?;
    let refused = inspect_scratch(engine.scratch.path(), engine.scratch_identity).is_err();
    fs::remove_dir(engine.scratch.path())?;
    fs::rename(held, engine.scratch.path())?;
    ensure!(refused, "replaced root was accepted");
    engine.cleanup()?;
    ensure!(fs::read(canary)? == b"retained", "link target was altered");
    outside.cleanup()?;
    Ok(())
}

#[test]
fn image_admission_requires_exact_pinned_platform_identity() {
    let manifest = BASE.split_once('@').unwrap().1;
    let mut base = json!({"Id":BASE_ID,"Os":"linux","Architecture":"amd64",
        "RepoDigests":[format!("ubuntu@{manifest}")]});
    assert!(validate_base(&base).is_ok());
    base["Id"] = json!(manifest);
    assert!(validate_base(&base).is_err());
    base["Descriptor"] =
        json!({"digest":manifest,"mediaType":"application/vnd.oci.image.manifest.v1+json"});
    assert!(validate_base(&base).is_ok());
    for (field, value) in [
        ("Id", json!("sha256:unknown")),
        ("Architecture", json!("arm64")),
        ("Os", json!("windows")),
        ("RepoDigests", json!([])),
        (
            "Descriptor",
            json!({"digest":manifest,"mediaType":"application/vnd.oci.image.index.v1+json"}),
        ),
        (
            "Descriptor",
            json!({"digest":"sha256:wrong","mediaType":"application/vnd.oci.image.manifest.v1+json"}),
        ),
    ] {
        let mut wrong = base.clone();
        wrong[field] = value;
        assert!(validate_base(&wrong).is_err(), "accepted wrong {field}");
    }
}
