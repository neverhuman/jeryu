# Agent sandbox image preparation

This recipe is **unqualified**. The required root OCI lane currently proves the
small OCI probe image; it does not build or qualify this coding-agent image.
Do not invoke the legacy smoke harness or publish this image until the artifact
and lifecycle requirements below are implemented and independently qualified.

The recipe now uses the monorepo root as its build context. The guard package
belongs to `components/jeryu-release-ops/crates/jeryu-git-guard`; the Web application
and UX package use the root npm workspace and package-lock.json. Cargo build and
fetch are locked, npm dependency failures abort the build, Jekko installation is
mandatory, and entrypoint HOME-cache creation failures prevent the agent from
starting. These source corrections do not prove a successful image build.

## Intended contents and runtime contract

The image must supply Rust with clippy/rustfmt, Node and the Web build tools,
Codex, Jekko, Claude, prefetched Cargo/npm dependencies, the authenticated pinned
Jankurai auditor, and the Jeryu Git guard. The guard is installed as `git` with
refusal wrappers for network, privilege, package-installation and host-management
tools. Credentials must be injected per run, never included in the build context
or image layers.

`OciSpec::from_agent_job` describes the runtime: read-only root, writable
`/tmp` and the assigned workspace, dropped capabilities, no-new-privileges,
the shipped seccomp profile, user 1000:1000, memory/PID limits and no network.
The image recipe alone does not establish that an engine enforced those options.

## Required artifacts before an image can qualify

| Input | Current recipe | Required evidence |
| --- | --- | --- |
| Source | Root-context COPY | Exact commit/tree and a verified committed-source context; no ambient untracked files, credentials, runtime state or build caches |
| Rust builder/runtime | Rust 1.95 image tag and rustup 1.95 default; root rust-toolchain.toml currently selects 1.97.1 | Qualified digest-bound builder and runtime toolchain consistent with the source; tool versions and offline build evidence |
| Runtime OS, APT and Node | Debian bookworm tag, live APT, downloaded rustup/NodeSource scripts and Node 22.x | Approved immutable distributions/checksums and dependency provenance; runtime compatibility and security scan |
| Agent tools | Unversioned npm packages @openai/codex, @anthropic-ai/claude-code and @jeryu/jekko-cli | Exact package versions, resolved artifact integrity and license/provenance; all three executable/version checks |
| Web tools/cache | Unversioned global Vite/TypeScript plus root locked workspace dependencies | Bound global-tool distributions and a real offline workspace build using the image cache |
| Auditor | Private-forge ordinary Cargo install with only a version comparison | Existing public-candidate build/verification plus separately qualified image placement and authority evidence; never a rewritten host receipt |
| Published image | No qualified registry or image digest established here | Owning publication authority, immutable digest, SBOM, signature and exact-source runtime receipt |

The Jekko package name above is only the recipe's current install coordinate.
No exact Jekko distribution or installation authority is established by that
name. This document does not assert that the package is publicly available or
that its absence is proven. Failing its installation now fails the image build.

The root `scripts/bootstrap-jankurai.sh` already calls the owning public-candidate
installer and verifier. That verifier checks the immutable builder/source pins,
golden binary digest, version, source and content-addressed receipt while holding
the binary descriptor and installation lock. An image preparation step must use
that existing admission, retain the actual receipt and revalidate the held binary
when copying it. Its candidate receipt does **not** authorize rewriting the
installation path or claiming the governed image installation succeeded. The
existing generated pin block remains untouched by this preparation change.

## Remaining real smoke and custody work

The legacy `ops/agent-sandbox/smoke.sh` retains all 21 checks: four filesystem
checks, one network check, four refusal-wrapper checks, one raw symlink check,
five Git checks, three toolchain checks and three auditor path/version checks.
It now fails if the engine is absent, arguments are unsupported, a check fails,
or the aggregate contains anything other than exactly 21 passes.

Those checks still do not qualify the image. Broad nonzero assertions can mistake
engine/tool failure for enforcement, non-root permission failures do not establish
a read-only mount, and path/version assertions do not authenticate the auditor.
The old harness also uses a shared image tag, unbounded engine commands and
unverified destructive cleanup. Keep it uninvoked while those defects remain.

Move the real 21-behavior proof into the existing OCI Engine's bounded, exact-ID
container lifecycle. Add tool-distribution, authenticated auditor, offline Cargo
and Web-build assertions. Bind every result to the exact source/profile/image;
record creation and exit identity, actual refusal causes, logs and successful
container removal. Preserve uncertain scratch and evidence for guarded retirement.
Only then make that shared owning command mandatory in local and hosted CI.
Missing capabilities, skipped proofs and incomplete counts must fail. The small
probe-image lane remains a separate required proof.

See [publication prerequisites](PUBLISH.md). No build, real smoke or publication
is authorized by this document.
