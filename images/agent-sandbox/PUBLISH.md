# Publishing the agent-sandbox image

The runner launches confined agent sessions from this image via
`OciSpec::from_agent_job` (crates/jeryu-runner-oci). It reads the image reference from
`JERYU_AGENT_IMAGE` (default `localhost/jeryu/agent-sandbox:latest`). To roll a new build
out to the fleet you build it, prove the lockdown holds, push it to the internal registry,
and point `JERYU_AGENT_IMAGE` at the pushed tag.

The engine is selected by `JERYU_OCI_RUNTIME` (default `podman`), matching the runner.

## 1. Build

```
podman build -f images/agent-sandbox/Dockerfile -t localhost/jeryu/agent-sandbox:latest .
```

The build needs network (rustup, NodeSource, npm) and the repo root as context — it copies
`Cargo.toml`/`Cargo.lock`, the git guard crate, the seccomp profile, the refusal wrapper,
and the web manifest to prefetch the toolchain and deps. The running container itself is
`--network none`, so everything an agent builds with must be baked in here.

## 2. Prove the lockdown on a real engine

```
ops/agent-sandbox/smoke.sh
```

The smoke builds the image (tagged `:smoke`) and runs the hardened container with the EXACT
flags the runner emits, asserting read-only root, `--network none`, the refusal wrappers,
the seccomp symlink block, the git guard, and the in-image toolchain. It prints
`agent-sandbox smoke: PASSED` on success and exits non-zero on any failed assertion. Where
no engine is on `PATH` it prints `SKIP` and exits 0, so it is safe to invoke anywhere.

Do NOT publish a build the smoke did not pass.

## 3. Tag + push to the internal registry

The internal registry lives at `registry.jeryu.internal`. Tag the proven build with an
immutable, content-addressed tag (a short commit sha or build date) plus a moving tag the
fleet tracks:

```
REG=registry.jeryu.internal/jeryu/agent-sandbox
REV="$(git rev-parse --short HEAD)"
podman tag localhost/jeryu/agent-sandbox:latest "$REG:$REV"
podman tag localhost/jeryu/agent-sandbox:latest "$REG:latest"
podman push "$REG:$REV"
podman push "$REG:latest"
```

Pin the fleet to the immutable `:$REV` tag once it is verified; the moving `:latest` tag is
for convenience only.

## 4. Point the runner at the published tag

The runner consumes the image through `JERYU_AGENT_IMAGE`. Set it to the pushed reference
so every confined session launches from the proven build:

```
JERYU_AGENT_IMAGE=registry.jeryu.internal/jeryu/agent-sandbox:<rev>
```

Leave it unset and the runner uses the local `localhost/jeryu/agent-sandbox:latest` build,
which is the right reference for a single-host dev or smoke loop.
