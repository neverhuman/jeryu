# Web bundle dependency notices

[Distributed notices](../../components/jeryu-web/apps/web/public/DEPENDENCY_NOTICES.txt)
accompany both the current Web application and the preserved historical
bundles identified in [the exact provenance catalog](web-bundles.json).
The application serves the same notice at /DEPENDENCY_NOTICES.txt.
[Font notices](../../components/jeryu-web/apps/web/public/THIRD_PARTY_NOTICES.txt)
remain separate. Jeryu's root Apache-2.0 license is unchanged.

## Evidence and scope

The six historical source maps in commits
01766cb8db3e8244ed3033a43511c42c8c3c6241 and
e71b55c4e485b88fb3f67c74a607e59a59fb646b contain 353 source entries.
All 205 third-party entries exactly match files from 45 official npm
package/version archives, verified against their historical lockfile
SHA-512 integrity values. The remaining 148 entries are application sources.

The six current maps observed after the production build from
7b9781b168a17c71d37199a2ebcb85b42afa7ce7 contain 377 entries.
All 200 third-party entries exactly match files from 43 locked npm archives;
the remaining 177 entries are application sources. The Web source tree and
lockfile blob, map hashes, each source/member hash, archive URL/integrity,
and exact covered package versions are recorded in the catalog.

There are 63 distinct runtime package/version identities across these two
snapshots. Four additional locked Vite/Rollup producer identities contribute
their complete upstream notice files for generated helper attribution.
Those producer entries make no source-map byte-match claim. This record is
not a license audit of every development dependency or external service.

The current application previously shipped only the font notice. Twenty of
the 45 historical package/version identities do not occur in the current
lock, so notices inferred from the historical package list alone would not
establish current coverage. Both snapshots were inspected independently.

## Notice acquisition

Most notices come directly from the exact integrity-verified npm archive.
This includes DOMPurify's full license/notice material and tslib's separate
CopyrightNotice.txt. The catalog gives each member path, original digest,
and offset/digest within the distributed notice. Text is deduplicated only
when original bytes are identical. Line endings and trailing horizontal
whitespace are normalized for source formatting; no notice words change.

Six historical Radix utility packages omit a notice in their npm archive.
Their exact published-version metadata binds the same tarball integrity to
gitHead fcef0668a5c827e5a4baac405474d75680f9a4eb. The full WorkOS MIT notice
comes from that immutable upstream revision:
[Radix license](https://raw.githubusercontent.com/radix-ui/primitives/fcef0668a5c827e5a4baac405474d75680f9a4eb/LICENSE).

react-remove-scroll-bar 2.3.8 also omits the license file. Its published
gitHead b3b1287aad81def2e2ae707274b74531b61ddbaf was unavailable upstream;
no v2.3.8 tag was found. All three source files embedded in both Jeryu
snapshots also exactly match the official integrity-verified 2.3.7 archive.
The unchanged MIT copyright/license wording is taken from
[upstream revision 8ca9ba5ea52de03308fe8ced94f7b159a44d28ff](https://raw.githubusercontent.com/theKashey/react-remove-scroll-bar/8ca9ba5ea52de03308fe8ced94f7b159a44d28ff/LICENSE),
whose package metadata declares 2.3.7/MIT. This supplies an explicitly
identified license source for identical bundled code; it does not resolve
or authenticate the missing 2.3.8 release commit.

The only third-party CSS import found in the application is xterm.css,
from the same covered xterm package. The historical compiled stylesheet
already retains its complete MIT comment. Other CSS is application source;
the fonts retain their own OFL notice.

## Maintaining and redistributing this record

When dependency versions or imported modules change, compare every emitted
source map, including lazy chunks, with the locked package archives, inspect
third-party CSS and generated helpers, and update this catalog and notices.
The runtime test checks that the approved notice bytes survive embedding
and source installation; it does not independently discover new license
obligations after a dependency update.

The additive record accompanies the existing Git graph without rewriting
commits or moving tags. If an old bundle/tree is redistributed on its own,
include the matching notices with that artifact. Current main does not
retroactively modify an old standalone archive.

This notice record does not approve publication, prove a reproducible build,
or substitute for source/private-data, dependency, SBOM and release checks.
