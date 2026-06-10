# agent-sandbox image

The **minimal, locked-down container** a confined coding agent runs in. It is the
image side of the sandbox-session flow: an operator clicks a repo → jeryu launches an
isolated runner from THIS image, checked out at latest `main` on a freshly-assigned
unique branch, and the agent works there and submits to jeryu for PR CI.

## What's inside (and nothing else)
- Pinned **Rust** toolchain (+ clippy, rustfmt) and **Node + Vite/TypeScript/React**.
- This repo's **prefetched build dependencies** (cargo + npm caches) so the first build
  is warm and no network is needed.
- The agent CLIs: **Codex, Jekko, Claude**.
- The pinned **jankurai** auditor (**1.6.10**, rev-locked) at
  `/opt/rust/cargo/bin/jankurai` — the runtime is `--network none`, so the auditor must
  ship in the image for the in-sandbox CI lanes to audit offline.
- The **`jeryu-git` guard installed as `git`** (deny-by-default allowlist — only
  branch-local activity on the assigned branch; no push/fetch/worktree/branch-create)
  plus **refusal wrappers** for `gh`/`curl`/`wget`/`ssh`/`scp`/`nc`.
- A non-root user `agent` (uid/gid 1000).

No SSH client, no docker CLI, no extra packages, **no credentials** (injected per-run by
`jeryu-agent-auth`).

## How it's locked down at runtime
`OciSpec::from_agent_job` (crates/jeryu-runner-oci) runs this image with:
`--read-only` root + `--tmpfs /tmp`, `--cap-drop=ALL`, `--security-opt no-new-privileges`,
`--security-opt seccomp=/opt/jeryu/seccomp/<name>.json` where `<name>` is the plan's
seccomp name (`oci-docker-phase4-seccomp`; see `seccomp/oci-docker-phase4-seccomp.json`),
`--user 1000:1000`, `--memory`/`--pids-limit` from the plan's cgroup caps,
`--network none`, and **ONLY** the workspace bind-mounted at `/workspace`. So the agent
can reach nothing on the host beyond its own writable workspace.

## Disabled tools — the agent JUST edits + builds + commits in its branch
A refusal wrapper is installed AS every tool the agent does not need for that, so it
exits 127 with a short explanation instead of working. The agent uses `cargo`/`rustc`/
`rustup`/`node`/`npm`/`npx`/`tsc`/`vite` to build and the **git guard** to commit — and
nothing else. Disabled:
- **symlinks** (`ln`) — edit the real files directly; never link out of the workspace
  (also denied at the `symlink`/`symlinkat` syscall level in
  `seccomp/oci-docker-phase4-seccomp.json`);
- networking: `gh curl wget ssh scp sftp nc ncat netcat telnet socat git-remote-http(s)`;
- privilege: `sudo su doas`;
- package installs: `apt apt-get aptitude dpkg pip pip3 gem` (the image is fixed; deps prefetched);
- containers/orchestration: `docker podman nerdctl kubectl ctr runc`;
- scheduling/daemons: `crontab at batch systemctl service`;
- mount/disk/namespace escape: `mount umount dd mkfs nsenter unshare chroot setarch`;
- keys/ownership/external-open: `ssh-keygen ssh-add gpg chown xdg-open open`.

The **git guard** (`jeryu-git` installed as `git`) separately forbids new branches /
switching branches / push / fetch / clone / worktrees — the agent can only commit, diff,
revert, etc. on its assigned branch and submit to jeryu for PR CI.

## How CI lanes find the auditor
Lanes always resolve jankurai via **`$JERYU_JANKURAI_BIN`** (baked into the image as
`/opt/rust/cargo/bin/jankurai`) or that explicit path — **never** a bare `jankurai`
PATH lookup, which a stale build earlier on PATH can shadow with the wrong version.
The path is exactly what `ops/ci/common.sh` derives from `CARGO_HOME`, so the lanes
run unchanged inside the sandbox. The pin (repo/rev/version) **must stay in sync with
`ops/ci/ensure-jankurai.sh`** — that script is the source of truth, and a pin bump
there requires rebuilding this image (the runtime has no network to install at session
time). The build and the smoke both assert the exact version string.

## Build
```
docker build -f images/agent-sandbox/Dockerfile -t localhost/jeryu/agent-sandbox:latest .
```
The runner references it via `JERYU_AGENT_IMAGE` (default
`localhost/jeryu/agent-sandbox:latest`). CI does not build it — the
`FakeContainerRuntime` exercises the run args without a daemon.

## Where this sits in the session flow
1. ✅ this image (light, locked-down) + `OciSpec::from_agent_job` (hardened run args)
2. ⬜ warm runner pool + claim → checkout latest `main` → assign a unique branch
3. ⬜ create-session API + TUI/web "New Session" button
4. ⬜ device-auth-once inheritance (credentials injected per run)
5. ⬜ mediated publish (agent commits → jeryu opens the PR → host-ci gate)
