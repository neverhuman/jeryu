# Public candidate auditor installation

From a clean monorepo, `bash scripts/ci.sh auditor` checks every generated
consumer, prepares the pinned Rust vendor tool, builds the immutable public
auditor source in Docker, and verifies a portable candidate installation.
The `legacy` lane invokes the same bootstrap before its retained proof commands.
This requires Linux x86_64, an unprivileged user with access to the local Docker
socket, two available CPUs, rustup, Git, jq, and the standard GNU shell tools.
Source installation of the Jeryu application uses its separate build/install
scripts and does not require an auditor installation.

The default audit tool destination is `$XDG_CACHE_HOME/jeryu/auditor`, falling
back to `$HOME/.cache/jeryu/auditor`. Set `JERYU_AUDITOR_INSTALL_ROOT` to an
explicit physical directory to override it. Each ancestor must have appropriate
ownership and write protection. The installed local-forge auditor and release
broker roots are refused. No private forge credential is read by this mode.

The installer can also be invoked directly:

```sh
JERYU_INSTALL_ROOT=/absolute/private/auditor \
  bash components/jeryu-tool/ops/install-jankurai.sh \
  --public-candidate --expected-head "$(git rev-parse HEAD)"
```

The candidate command rejects caller pins, test/prebuilt inputs, credentials,
and ambient Git/askpass overrides. It anonymously verifies the immutable public
tag, acquires a fresh locked crates.io cache, then builds offline with the exact
vendor, toolchain, image, source, context and golden binary identities. The
original producer URL remains recorded separately from public transport.
The final stdout line is compact JSON containing `status`, `receipt`, `path`,
and `sha256`; diagnostic build output may precede it.

Installation retains the existing exclusive lock, descriptor custody, atomic
replacement and verified rollback transaction. Candidate lock contention fails
after 30 seconds. Scratch deletion checks directory identity, mounts and links;
changed scratch is retained. Failed build diagnostics remain under the private
installation root. The command never writes a protected API-v2 receipt.

The distinct `jeryu.jankurai-public-candidate-installation/v1` receipt binds the
complete current renderer output, immutable producer/distribution identities,
committed pin/builder inputs, actual build, installed binary and lock. It records
`protected_main=false`, `handover=pending`, and `test_mode=false`. This is local
source/build evidence; it does not authenticate a protected predecessor or
qualify a release or authority handover.

CI recomputes clean-source metadata, validates the closed receipt and all input
digests, and executes the auditor through a retained file descriptor while
holding a shared installation lock. Repeated bootstrap for the same source and
destination reverifies that installation. A different selection held by the
current process or its parent requires a fresh qualification process.
The default protected installer and release-broker verification remain separate.

The Linux Rust tests cover candidate receipt/authority substitution and file
custody. Retained shell tests exercise protected installation, interruption,
concurrent transactions, rollback, renderer custody and root-seal restoration.
Actual candidate installation and final-source CI qualification are recorded
separately in the monorepo migration status.
