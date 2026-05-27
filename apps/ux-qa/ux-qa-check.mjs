#!/usr/bin/env node
// ux-qa-check.mjs — proof collector for the JeRyu Web Forge (W-T-19).
//
// Phase 0 of this file was a marker-string check against `ux-qa.{ts,md}`.
// Phase 7 (this revision) upgrades it to a real proof collector that
// verifies the production-grade UX-QA artifacts exist and pass their
// thresholds. The checks are intentionally permissive when a single
// artifact is missing — we exit `1` with a diagnostic so CI fails loudly,
// but the per-check status is preserved in the JSON receipt so reviewers
// can see which lane is unfinished.
//
// Checks (in this order):
//   1. Vite build outputs           apps/web/dist/index.html + assets/
//   2. Storybook build              apps/web/storybook-static/index.html
//   3. Playwright report            apps/web/playwright-report/index.html
//   4. axe scan receipts            target/jankurai/ux-qa/*.axe.json with
//                                     zero `critical`/`serious` violations
//   5. Markdown XSS fixture          target/jankurai/ux-qa/markdown-xss.json
//                                     (runs `cargo nextest run -p jeryu
//                                     --test web_markdown_tests` to mint it
//                                     when missing)
//   6. WS replay test               Playwright report contains spec
//                                     `08-ws-reconnect`
//   7. Bundle size budget           gzip(dist/assets/index-*.js) < 350 KB
//   8. Receipt                      target/jankurai/ux-qa/web-forge.<ISO>.json
//
// Output: a top-level `pass: bool` plus per-check `pass: bool` and
// optional `details`. Exit code 0 when all critical checks pass, 1 if
// any required check fails. A missing artifact for a still-pending lane
// still triggers exit 1 with `reason: 'artifact missing'`.

import { spawnSync } from 'node:child_process';
import { createGzip } from 'node:zlib';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const workspaceDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(workspaceDir, '..', '..');
const mode = process.argv[2] ?? 'build';
if (!['build', 'test'].includes(mode)) {
  console.error('usage: node ux-qa-check.mjs <build|test>');
  process.exit(2);
}

const webDir = join(repoRoot, 'apps', 'web');
const uxArtifactDir = join(repoRoot, 'target', 'jankurai', 'ux-qa');
mkdirSync(uxArtifactDir, { recursive: true });

const BUNDLE_BUDGET_BYTES = 350 * 1024;

// ── helper utilities ───────────────────────────────────────────────────────

function digest(value) {
  return createHash('sha256').update(value).digest('hex');
}

function check(name, fn) {
  try {
    return Promise.resolve(fn()).then((value) => normalizeCheck(name, value));
  } catch (err) {
    return Promise.resolve(
      normalizeCheck(name, {
        pass: false,
        details: { error: err.message ?? String(err) },
      })
    );
  }
}

function normalizeCheck(name, value) {
  if (!value || typeof value !== 'object') {
    return { name, pass: false, details: { reason: 'no result' } };
  }
  return { name, pass: Boolean(value.pass), details: value.details ?? null };
}

async function gzipByteCount(filePath) {
  const buf = readFileSync(filePath);
  const chunks = [];
  await new Promise((resolveP, rejectP) => {
    const gz = createGzip({ level: 9 });
    gz.on('data', (c) => chunks.push(c));
    gz.on('end', resolveP);
    gz.on('error', rejectP);
    gz.end(buf);
  });
  return Buffer.concat(chunks).length;
}

// ── individual checks ──────────────────────────────────────────────────────

function checkViteBuild() {
  const indexHtml = join(webDir, 'dist', 'index.html');
  const assetsDir = join(webDir, 'dist', 'assets');
  if (!existsSync(indexHtml)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/dist/index.html' },
    };
  }
  if (!existsSync(assetsDir)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/dist/assets/' },
    };
  }
  const entries = readdirSync(assetsDir);
  return {
    pass: true,
    details: {
      index_html: indexHtml,
      asset_count: entries.length,
    },
  };
}

function checkStorybookBuild() {
  const indexHtml = join(webDir, 'storybook-static', 'index.html');
  if (!existsSync(indexHtml)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/storybook-static/index.html' },
    };
  }
  return { pass: true, details: { index_html: indexHtml } };
}

function checkPlaywrightReport() {
  const indexHtml = join(webDir, 'playwright-report', 'index.html');
  if (!existsSync(indexHtml)) {
    return {
      pass: false,
      details: {
        reason: 'missing apps/web/playwright-report/index.html',
        hint: 'Run `npm --workspace @jeryu/web run test:e2e`',
      },
    };
  }
  return { pass: true, details: { index_html: indexHtml } };
}

function checkAxeScans() {
  // Accept either `playwright-axe-<page>.json` (named per page) or any
  // `*.axe.json` artifact under the UX-QA target dir.
  if (!existsSync(uxArtifactDir)) {
    return {
      pass: false,
      details: { reason: `missing ${uxArtifactDir}` },
    };
  }
  const candidates = readdirSync(uxArtifactDir).filter(
    (f) => f.endsWith('.axe.json') || /playwright-axe-.*\.json$/.test(f)
  );
  if (candidates.length === 0) {
    return {
      pass: false,
      details: {
        reason: 'no axe scan artifacts in target/jankurai/ux-qa/',
        hint: 'Run `npm --workspace @jeryu/web run test:e2e` (10-a11y.spec.ts emits these)',
      },
    };
  }
  const failures = [];
  const fileSummaries = [];
  for (const fname of candidates) {
    const fpath = join(uxArtifactDir, fname);
    try {
      const json = JSON.parse(readFileSync(fpath, 'utf8'));
      const violations = Array.isArray(json.violations) ? json.violations : [];
      const offenders = violations.filter(
        (v) => v.impact === 'critical' || v.impact === 'serious'
      );
      fileSummaries.push({
        file: fname,
        total_violations: violations.length,
        critical_or_serious: offenders.length,
      });
      for (const o of offenders) {
        failures.push({ file: fname, rule: o.id, impact: o.impact });
      }
    } catch (err) {
      failures.push({ file: fname, error: err.message });
    }
  }
  return {
    pass: failures.length === 0,
    details: { files: fileSummaries, failures },
  };
}

function checkMarkdownXss() {
  const fixturePath = join(uxArtifactDir, 'markdown-xss.json');
  if (existsSync(fixturePath)) {
    try {
      const json = JSON.parse(readFileSync(fixturePath, 'utf8'));
      return { pass: Boolean(json.pass), details: { file: fixturePath, ...json } };
    } catch (err) {
      return {
        pass: false,
        details: { reason: 'markdown-xss.json present but unparseable', error: err.message },
      };
    }
  }
  // Mint the fixture from the existing Rust test. We invoke `cargo
  // nextest` because it is the canonical runner the rest of the repo
  // uses; fall back to `cargo test` if nextest is unavailable.
  const useNextest = spawnSync('cargo', ['nextest', '--version'], {
    cwd: repoRoot,
    stdio: 'ignore',
  }).status === 0;
  const args = useNextest
    ? ['nextest', 'run', '-p', 'jeryu', '--test', 'web_markdown_tests', '--no-fail-fast']
    : ['test', '-p', 'jeryu', '--test', 'web_markdown_tests', '--no-fail-fast'];
  const start = Date.now();
  const result = spawnSync('cargo', args, {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
  });
  const elapsedMs = Date.now() - start;
  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  const passed = result.status === 0;
  const receipt = {
    pass: passed,
    runner: useNextest ? 'cargo nextest' : 'cargo test',
    args,
    elapsed_ms: elapsedMs,
    exit_code: result.status,
    stdout_tail: stdout.split('\n').slice(-20).join('\n'),
    stderr_tail: stderr.split('\n').slice(-20).join('\n'),
    generated_at: new Date().toISOString(),
  };
  writeFileSync(fixturePath, JSON.stringify(receipt, null, 2) + '\n');
  return {
    pass: passed,
    details: { file: fixturePath, ...receipt },
  };
}

function checkWsReplay() {
  // We look for the `08-ws-reconnect` spec in the Playwright HTML report.
  // The simpler proxy is `playwright-report/data/<hash>.json` which lists
  // every spec run; if it doesn't exist, scan index.html for the spec name.
  const reportDir = join(webDir, 'playwright-report');
  if (!existsSync(reportDir)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/playwright-report/' },
    };
  }
  const indexHtml = join(reportDir, 'index.html');
  if (!existsSync(indexHtml)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/playwright-report/index.html' },
    };
  }
  const html = readFileSync(indexHtml, 'utf8');
  if (!html.includes('08-ws-reconnect')) {
    return {
      pass: false,
      details: {
        reason: 'spec 08-ws-reconnect not referenced in Playwright report',
        hint: 'Re-run `npm --workspace @jeryu/web run test:e2e`',
      },
    };
  }
  return { pass: true, details: { report: indexHtml } };
}

async function checkBundleSize() {
  const assetsDir = join(webDir, 'dist', 'assets');
  if (!existsSync(assetsDir)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/dist/assets/' },
    };
  }
  const jsFiles = readdirSync(assetsDir).filter((f) => f.endsWith('.js'));
  let totalGz = 0;
  const perFile = [];
  for (const f of jsFiles) {
    const fpath = join(assetsDir, f);
    const raw = statSync(fpath).size;
    const gz = await gzipByteCount(fpath);
    totalGz += gz;
    perFile.push({ file: f, raw_bytes: raw, gzip_bytes: gz });
  }
  return {
    pass: totalGz < BUNDLE_BUDGET_BYTES,
    details: {
      total_gzip_bytes: totalGz,
      budget_bytes: BUNDLE_BUDGET_BYTES,
      per_file: perFile,
    },
  };
}

// ── runner ─────────────────────────────────────────────────────────────────

const checks = [
  ['vite_build', checkViteBuild],
  ['storybook_build', checkStorybookBuild],
  ['playwright_report', checkPlaywrightReport],
  ['axe_scans', checkAxeScans],
  ['markdown_xss', checkMarkdownXss],
  ['ws_replay', checkWsReplay],
  ['bundle_size', checkBundleSize],
];

const results = [];
for (const [name, fn] of checks) {
  // eslint-disable-next-line no-await-in-loop
  const result = await check(name, fn);
  results.push(result);
}

const passed = results.every((r) => r.pass);
const isoStamp = new Date().toISOString().replace(/[:.]/g, '-');
const receiptPath = join(uxArtifactDir, `web-forge.${isoStamp}.json`);
const stableReceiptPath = join(uxArtifactDir, 'web-forge.latest.json');

const receipt = {
  pass: passed,
  mode,
  generated_at: new Date().toISOString(),
  repo_root: repoRoot,
  checks: results,
  evidence: {
    'ux-qa.ts': digest(readFileSync(join(workspaceDir, 'ux-qa.ts'), 'utf8')),
    'ux-qa.md': digest(readFileSync(join(workspaceDir, 'ux-qa.md'), 'utf8')),
  },
};

writeFileSync(receiptPath, JSON.stringify(receipt, null, 2) + '\n');
writeFileSync(stableReceiptPath, JSON.stringify(receipt, null, 2) + '\n');

console.log(`ux-qa ${mode}: ${passed ? 'OK' : 'FAIL'}`);
for (const r of results) {
  console.log(`  - ${r.name}: ${r.pass ? 'pass' : 'fail'}`);
  if (!r.pass && r.details?.reason) {
    console.log(`      reason: ${r.details.reason}`);
  }
}
console.log(`receipt: ${receiptPath}`);

if (!passed) {
  process.exit(1);
}
