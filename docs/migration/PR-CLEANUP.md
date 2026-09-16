# Dependabot PR cleanup register

Baseline `main`: `72a8a868494c95e7e08c6f52bf461a14cd4d8a6d` (2026-09-15).
Campaign replacement: PR **#94** merged to `main` as `b67b99fff9dd` (tree of `74b0966c`).
Dependabot branch refs preserved as `refs/remotes/github/dependabot/*` and `pull/*/head`.

This register maps every Dependabot PR from the 2026-09-15 flood, plus #65/#66 preservation.

Columns: original head; packet; failure class; disposition.

## Historical preservation

| PR | Original head | Intent | Disposition |
| --- | --- | --- | --- |
| 67 | `75d7966b` rebased to `c2cccbc2` then `72a8a868` | Public SoT, 7-lane gate | **Merged** to `main` |
| 69 | `586069de` rebased to `72a8a868` | Required-only Actions, nightly advisory | **Merged** to `main` |
| 65 | `b50dc1f1` | Public candidate close-out | Closed superseded by #67. Unique SHAs are not ancestors (GitHub rebase-merge). |
| 66 | `791bd393` | Release gates / recoverable repo creation | Closed unmerged as a PR, **accepted work is on `main`**: `repository_creation.rs` and `0013_repository_creation.sql` arrived via `b403c61b` (“Reconcile foundation candidate onto PR65 with complete corrective gates”). |

## Packet A — Rust (hosted `jeryu/required` green on originals)

| PR | Head | Bump | Failure | Disposition |
| --- | --- | --- | --- | --- |
| 76 | `ae625d3b` | base64 0.22.1 → 0.23.1 | none | In #94 |
| 78 | `769a75cc` | uuid 1.26.0 → 1.26.1 | none | In #94 |
| 79 | `3294d8c2` | rand 0.8.8 → 0.9.5 | none (major; tests passed) | In #94 |
| 83 | `a2e072ff` | landlock 0.4.5 → 0.4.7 | none | In #94 |
| 84 | `a3227c32` | nix 0.29.0 → 0.31.3 | none (major; tests passed) | In #94 |
| 87 | `38617ce9` | thiserror 1.0.69 → 2.0.20 | none (major; tests passed) | In #94 |
| 91 | `58a02675` | ed25519-dalek 2.2.0 → 3.0.0 | none (major; tests passed) | In #94 |

Originals closed as superseded **before** #94 merged (process miss vs campaign rule). Branch refs retained. Close is valid only after #94 `jeryu/required` is green on resulting `main` and lockfile contains those versions.

## Packet B — Rust failing upgrades (repair, do not drop)

| PR | Head | Bump | Failure class | Repair |
| --- | --- | --- | --- | --- |
| 81 | `cabde66b` | sha2 0.10.9 → 0.11.0 | Upgrade regression | `hmac` 0.12 cannot use sha2 0.11 (`CoreProxy`). Bump workspace `hmac` to 0.13 with sha2 0.11. |
| 89 | `592239e3` | toml 0.8.23 → 1.1.6 | Upgrade regression | `jeryu-wsversion` `cargo_source_contract` TOML parse/table API. Adapt parser, keep git/package/tag contract. |
| 93 | `cb092609` | reqwest 0.12.28 → 0.13.5 | Upgrade / policy | `cargo deny` rejects **ISC** and **CDLA-Permissive-2.0** (aws-lc). Either allow those OSI-permissive licenses or keep 0.12. Do not silently disable deny. |

## Packet C — Web build stack

| PR | Head | Bump | Failure class | Repair |
| --- | --- | --- | --- | --- |
| 70 | `d62a256b` | esbuild + vite + storybook group | Behind `main`; TS2769 `manualChunks` object form | Convert `vite.config.ts` to function `manualChunks(id)` preserving vendor groups. Behind-main: rebase onto post-#94. |
| 75 | `0c21b406` | plugin-react 5.2.0 → 6.1.1 | Coupled to Vite 8 | Land with Vite 8, not alone. |
| 82 | `05862550` | vite 6.4.3 → 8.3.0 | Same TS2769 + rust/web/runtime | Packet C combo. |
| 85 | `20a19dd1` | storybook 10.4.1 → 10.6.0 | Inventory drift only | Already in #94 (10.6.0). |

## Packet D — React family

| PR | Head | Bump | Failure class | Repair |
| --- | --- | --- | --- | --- |
| 80 | `8e30f5d6` | react 19.2.6 → 19.3.0 | Upgrade: 25 component test files failed | Repair tests/hooks after C; keep 19.2.6 if repair is a behavior change. |
| 90 | `bcf7b441` | react-dom 19.2.6 → 19.3.0 | Pair with 80 | Same. |
| 88 | `d9920aad` | testing-library 16.3.2 → 16.3.3 | Inventory drift | In #94. |

## Packet E — Web libraries

| PR | Head | Bump | Failure class | Repair |
| --- | --- | --- | --- | --- |
| 74 | `637b729e` | react-virtual 3.13.26 → 3.14.12 | Inventory drift (`jeryu-split` bin), not product rust | Include after inventory regen. |
| 77 | `b404f15a` | zod 4.4.3 → 4.6.2 | Inventory drift | In #94. |
| 86 | `465d501e` | react-query 5.100.14 → 5.102.8 | Inventory drift | In #94. |
| 92 | `1c4e95b1` | react-table 8.21.3 → 9.2.4 | TS7006/7031 untyped `row` in `RepoTable.tsx` | Explicit `Row<RepositorySummary>` cell args. Last in E. |

## Packet F — GitHub Actions

| PR | Head | Bump | Failure class | Repair |
| --- | --- | --- | --- | --- |
| 71 | `465e65e4` | setup-node 4 → 7 | Inventory drift | In #94 SHA pin `82076278…` |
| 72 | `624a6cd9` | upload-artifact 4 → 7 | Inventory drift | In #94 SHA pin `043fb46d…` |
| 73 | `903534b5` | checkout 4 → 7 | Inventory drift | In #94 SHA pin `3d3c42e5…` |

## Merge order (remaining after #94)

1. #94 required-green → resulting `main` (grouped A + safe E + F; lockfiles make separate sequential Dependabot merges unsafe).
2. Packet B: hmac 0.13 + sha2 0.11; toml 1.x contract; reqwest 0.13 + deny license decision.
3. Packet C: Vite 8 + plugin-react 6 + esbuild 0.28.2 + function `manualChunks`.
4. Packet D: React 19.3 only if tests pass without weakening lint.
5. Packet E remainder: virtual 3.14, then table v9.
6. Stop if resulting `main` `jeryu/required` fails.

## Closed-before-replacement (process)

Originals 70–93 were closed during an earlier empty-queue pass. This campaign treats those closes as **not** final until replacements merge. Branch refs are preserved on GitHub (`dependabot/*`) and locally (`refs/remotes/github/dependabot/*`, `refs/remotes/github/pr-{65,66}`).
