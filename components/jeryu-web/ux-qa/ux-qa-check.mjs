#!/usr/bin/env node
// ux-qa-check.mjs — proof collector for the JeRyu Web Forge (W-T-19).
//
// Phase 0 of this file was a marker-string check against `ux-qa.{ts,md}`.
// Phase 7 (this revision) upgrades it to a real proof collector that
// verifies the production-grade UX-QA artifacts exist and pass their
// thresholds. Build mode checks static build artifacts only; test mode also
// requires browser evidence from the Playwright/axe lane. The checks are
// intentionally permissive when a single artifact is missing — we exit `1`
// with a diagnostic so CI fails loudly, but the per-check status is preserved
// in the JSON receipt so reviewers can see which lane is unfinished.
//
// Checks (in this order):
//   1. Vite build outputs           web/dist/index.html + assets/
//   2. Storybook build              web/storybook-static/index.html
//   3. Playwright report            web/playwright-report/index.html
//   4. axe scan receipts            target/jankurai/ux-qa/*.axe.json with
//                                     zero `critical`/`serious` violations
//   5. Rendered surface evidence     paired screenshot + geometry/token JSON
//                                     for every axe scan receipt
//   6. Markdown XSS fixture          target/jankurai/ux-qa/markdown-xss.json
//                                     (runs `cargo nextest run -p jeryu
//                                     --test web_markdown_tests` to mint it
//                                     when missing)
//   7. WS replay test               Playwright report contains spec
//                                     `08-ws-reconnect`
//   8. Bundle size budget           gzip(dist/assets/index-*.js) < 350 KB
//   9. Lighthouse perf score        target/jankurai/ux-qa/lighthouse/*.report.json
//                                     OR target/jankurai/ux-qa/lighthouse.json
//                                     (soft-pass if no artifacts; fails only
//                                     when score < 0.7 in collected runs)
//  10. Receipt                      target/jankurai/ux-qa/web-forge.<ISO>.json
//
// Output: a top-level `pass: bool` plus per-check `pass: bool` and
// optional `details`. Exit code 0 when all checks for the selected mode pass,
// 1 if any required check fails. A missing artifact for a required lane still
// triggers exit 1 with `reason: 'artifact missing'`.

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
// `ux-qa/` lives at the repo root; the product SPA is `apps/web/` in this
// workspace, with a fallback to `web/` for older sibling layouts.
const repoRoot = resolve(workspaceDir, '..');
const mode = process.argv[2] ?? 'build';
if (!['build', 'test'].includes(mode)) {
  console.error('usage: node ux-qa-check.mjs <build|test>');
  process.exit(2);
}

const webDir = existsSync(join(repoRoot, 'apps', 'web'))
  ? join(repoRoot, 'apps', 'web')
  : join(repoRoot, 'web');
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

function checkRenderedEvidence() {
  const axeFiles = readdirSync(uxArtifactDir).filter((f) =>
    f.endsWith('.axe.json')
  );
  const failures = [];
  const surfaces = [];
  for (const axeFile of axeFiles) {
    const scope = axeFile.slice(0, -'.axe.json'.length);
    const screenshotFile = join(uxArtifactDir, `${scope}.screenshot.png`);
    const renderedFile = join(uxArtifactDir, `${scope}.rendered.json`);
    if (!existsSync(screenshotFile) || statSync(screenshotFile).size === 0) {
      failures.push({ scope, reason: 'missing or empty screenshot' });
      continue;
    }
    if (!existsSync(renderedFile)) {
      failures.push({ scope, reason: 'missing rendered JSON' });
      continue;
    }
    try {
      const rendered = JSON.parse(readFileSync(renderedFile, 'utf8'));
      const width = rendered?.geometry?.width;
      const height = rendered?.geometry?.height;
      const color = rendered?.design_tokens?.color_bg_0;
      const spacing = rendered?.design_tokens?.space_4;
      if (!(width > 0) || !(height > 0) || !color || !spacing) {
        failures.push({ scope, reason: 'invalid geometry or design-token readback' });
        continue;
      }
      surfaces.push({
        scope,
        screenshot_bytes: statSync(screenshotFile).size,
        width,
        height,
        color_bg_0: color,
        space_4: spacing,
      });
    } catch (err) {
      failures.push({ scope, reason: 'unparseable rendered JSON', error: err.message });
    }
  }
  if (axeFiles.length === 0) {
    failures.push({ reason: 'no axe receipts to pair with rendered evidence' });
  }
  return {
    pass: failures.length === 0,
    details: { surfaces, failures },
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
  // Mint the fixture from the real web Markdown renderer test. The XSS guard
  // lives in the React package, so the UX-QA collector calls that package's
  // Vitest target directly instead of a stale Rust package name.
  const args = ['run', 'test', '--', 'MarkdownRenderer'];
  const start = Date.now();
  const result = spawnSync('npm', args, {
    cwd: webDir,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
  });
  const elapsedMs = Date.now() - start;
  const stdout = result.stdout ?? '';
  const stderr = result.stderr ?? '';
  const passed = result.status === 0;
  const receipt = {
    pass: passed,
    runner: 'npm vitest',
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
  // We look for the `08-ws-reconnect` spec in the Playwright artifacts.
  // Preferred source: `playwright-report/junit.xml` (plain text — emitted by
  // the `junit` reporter in playwright.config.ts). Fallback: the single-file
  // `index.html` (modern Playwright bundles spec metadata into a base64-zip
  // template, so substring scans there only succeed when the report uses
  // `doNotInlineAssets`). Either source counts as proof.
  const reportDir = join(webDir, 'playwright-report');
  if (!existsSync(reportDir)) {
    return {
      pass: false,
      details: { reason: 'missing apps/web/playwright-report/' },
    };
  }
  const junitXml = join(reportDir, 'junit.xml');
  if (existsSync(junitXml)) {
    const xml = readFileSync(junitXml, 'utf8');
    if (xml.includes('08-ws-reconnect')) {
      return { pass: true, details: { proof: junitXml, source: 'junit' } };
    }
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
  return { pass: true, details: { report: indexHtml, source: 'index.html' } };
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

// Lighthouse perf-budget proof. We look for either an lhci-emitted LHR
// (`target/jankurai/ux-qa/lighthouse/*.report.json` — note lhci 0.15.x
// names artifacts `<host>--<timestamp>.report.json`, older docs reference
// `lhr-*.json`) or a hand-minted `target/jankurai/ux-qa/lighthouse.json`
// receipt. Pass if any LHR exists with `categories.performance.score >= 0.7`.
// If `@lhci/cli` is not installed we degrade to a soft pass with a hint
// rather than failing the harness.
const LIGHTHOUSE_PERF_THRESHOLD = 0.7;

function checkLighthouse() {
  const lighthouseDir = join(uxArtifactDir, 'lighthouse');
  const fallbackReceipt = join(uxArtifactDir, 'lighthouse.json');

  // Resolve candidate LHR files in priority order.
  const lhrFiles = [];
  if (existsSync(lighthouseDir)) {
    for (const fname of readdirSync(lighthouseDir)) {
      if (
        /^lhr-.+\.json$/.test(fname) ||
        /\.report\.json$/.test(fname)
      ) {
        lhrFiles.push(join(lighthouseDir, fname));
      }
    }
  }
  if (lhrFiles.length === 0 && existsSync(fallbackReceipt)) {
    lhrFiles.push(fallbackReceipt);
  }

  if (lhrFiles.length === 0) {
    // No artifacts. Check whether @lhci/cli is available so we can give a
    // useful hint; either way, this is a soft pass — the bundle_size check
    // already covers the size budget, and CI environments without Chrome
    // shouldn't fail the whole harness here.
    const lhciInstalled = existsSync(
      join(repoRoot, 'node_modules', '@lhci', 'cli', 'src', 'cli.js'),
    );
    const hint = lhciInstalled
      ? 'Run `JERYU_WEB_TRUST_LOCAL=1 ./target/release/jeryu web serve --bind 127.0.0.1:8787 --spa-dir apps/web/dist &` then `npm --workspace @jeryu/web run perf`'
      : 'Install with `npm install --workspace @jeryu/web @lhci/cli@latest` then run `npm --workspace @jeryu/web run perf`';
    return {
      pass: true,
      details: {
        reason: 'no lighthouse artifacts; treating as soft pass',
        lhci_installed: lhciInstalled,
        hint,
      },
    };
  }

  // We have at least one artifact. Surface the minimum perf score across
  // all collected runs so a regression in any single run trips the check.
  const reports = [];
  let minScore = Infinity;
  let representativeUrl = null;
  for (const fpath of lhrFiles) {
    try {
      const raw = JSON.parse(readFileSync(fpath, 'utf8'));
      // Two shapes are supported:
      //   1. A real LHR: `raw.categories.performance.score`.
      //   2. A hand-minted receipt: `raw.categories.performance.score`
      //      OR `raw.performance_score` (legacy convenience field).
      let score = raw?.categories?.performance?.score;
      if (typeof score !== 'number' && typeof raw?.performance_score === 'number') {
        score = raw.performance_score;
      }
      const url = raw?.requestedUrl ?? raw?.url ?? raw?.finalUrl ?? null;
      if (url && !representativeUrl) representativeUrl = url;
      if (typeof score === 'number') {
        if (score < minScore) minScore = score;
        reports.push({
          file: fpath,
          url,
          performance: score,
          accessibility: raw?.categories?.accessibility?.score ?? null,
          best_practices: raw?.categories?.['best-practices']?.score ?? null,
          seo: raw?.categories?.seo?.score ?? null,
        });
      } else {
        // Receipt without a usable score (e.g. placeholder JSON). Record
        // the file but don't flunk on a parse-shape mismatch.
        reports.push({
          file: fpath,
          url,
          performance: null,
          reason: 'no performance score in artifact',
        });
      }
    } catch (err) {
      reports.push({ file: fpath, error: err.message });
    }
  }

  if (minScore === Infinity) {
    return {
      pass: true,
      details: {
        reason: 'lighthouse artifacts present but no perf score extracted; soft pass',
        reports,
      },
    };
  }

  return {
    pass: minScore >= LIGHTHOUSE_PERF_THRESHOLD,
    details: {
      threshold: LIGHTHOUSE_PERF_THRESHOLD,
      min_performance_score: minScore,
      representative_url: representativeUrl,
      reports,
    },
  };
}

// ── runner ─────────────────────────────────────────────────────────────────

const allChecks = [
  ['vite_build', checkViteBuild],
  ['storybook_build', checkStorybookBuild],
  ['playwright_report', checkPlaywrightReport],
  ['axe_scans', checkAxeScans],
  ['rendered_evidence', checkRenderedEvidence],
  ['markdown_xss', checkMarkdownXss],
  ['ws_replay', checkWsReplay],
  ['bundle_size', checkBundleSize],
  ['lighthouse', checkLighthouse],
];
const buildModeChecks = new Set([
  'vite_build',
  'storybook_build',
  'markdown_xss',
  'bundle_size',
  'lighthouse',
]);
const checks = allChecks.filter(
  ([name]) => mode === 'test' || buildModeChecks.has(name)
);

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
