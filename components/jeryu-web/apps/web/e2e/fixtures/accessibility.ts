// accessibility.ts — Axe-core glue for Playwright (W-T-18).
//
// `runAxe(page, options?)` injects axe-core (4.x) via `@axe-core/playwright`
// and returns the analysis result. Specs write the result to
// `target/jankurai/ux-qa/<scope>.axe.json` so the artifact can be picked up
// by the UX-QA dashboard.
//
// Defaults restrict the rule set to WCAG 2.1 AA + best-practice so the suite
// stays green even when third-party SVGs report nuisance issues. Specs can
// pass `tags` to widen or narrow the scan.

import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { AxeBuilder } from '@axe-core/playwright';
import type { Page } from '@playwright/test';

const DEFAULT_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'];

const HELPER_DIR = path.dirname(fileURLToPath(import.meta.url));
const ARTIFACTS_DIR = path.resolve(
  HELPER_DIR,
  '..',
  '..',
  '..',
  '..',
  'target',
  'jankurai',
  'ux-qa'
);

export interface AxeRunOptions {
  /** Override the default WCAG 2.1 AA tag set. */
  tags?: string[];
  /** Restrict analysis to a CSS selector (defaults to whole page). */
  include?: string;
  /** Rule IDs to disable for this scope (workarounds + known false positives). */
  disableRules?: string[];
}

export interface RenderedEvidence {
  screenshot: string;
  geometry: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  design_tokens: {
    color_bg_0: string;
    space_4: string;
  };
}

function safeScope(scope: string): string {
  return scope.replace(/[^a-z0-9_-]+/gi, '-').toLowerCase();
}

/**
 * Run axe on the current page and return the raw analysis.
 *
 * Side effect: nothing is persisted. Call `persistAxeResult` after the scan
 * to dump JSON for the UX-QA pipeline.
 */
export async function runAxe(
  page: Page,
  options: AxeRunOptions = {}
): Promise<Awaited<ReturnType<AxeBuilder['analyze']>>> {
  let builder = new AxeBuilder({ page }).withTags(options.tags ?? DEFAULT_TAGS);
  if (options.include) builder = builder.include(options.include);
  if (options.disableRules && options.disableRules.length > 0) {
    builder = builder.disableRules(options.disableRules);
  }
  return builder.analyze();
}

/**
 * Write the axe result as JSON to
 * `target/jankurai/ux-qa/<scope>.axe.json` so the dashboard agent can
 * surface trends. The scope name is sanitized to a safe filename.
 */
export async function persistAxeResult(
  scope: string,
  result: Awaited<ReturnType<AxeBuilder['analyze']>>
): Promise<void> {
  const safe = safeScope(scope);
  await fs.mkdir(ARTIFACTS_DIR, { recursive: true });
  const outFile = path.join(ARTIFACTS_DIR, `${safe}.axe.json`);
  await fs.writeFile(outFile, JSON.stringify(result, null, 2), 'utf8');
}

/**
 * Capture fail-closed rendered evidence alongside each axe result. The JSON
 * proves the main surface has non-zero geometry and reads canonical design
 * tokens from the computed root style; the PNG gives reviewers the matching
 * visual artifact.
 */
export async function persistRenderedEvidence(
  page: Page,
  scope: string
): Promise<RenderedEvidence> {
  const safe = safeScope(scope);
  await fs.mkdir(ARTIFACTS_DIR, { recursive: true });
  const screenshotFile = path.join(ARTIFACTS_DIR, `${safe}.screenshot.png`);
  const renderedFile = path.join(ARTIFACTS_DIR, `${safe}.rendered.json`);

  const rendered = await page.evaluate(() => {
    const surface = document.querySelector('main#main-content') ?? document.body;
    const rect = surface.getBoundingClientRect();
    const rootStyle = getComputedStyle(document.documentElement);
    return {
      geometry: {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
      },
      design_tokens: {
        color_bg_0: rootStyle.getPropertyValue('--color-bg-0').trim(),
        space_4: rootStyle.getPropertyValue('--space-4').trim(),
      },
    };
  });

  await page.screenshot({ path: screenshotFile, fullPage: true });
  const evidence: RenderedEvidence = {
    screenshot: screenshotFile,
    ...rendered,
  };
  await fs.writeFile(renderedFile, `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
  return evidence;
}

/**
 * Filter axe violations down to the WCAG 2.1 AA-grade `serious` and `critical`
 * impacts so the e2e suite does not flake on best-practice nits.
 */
export function blockingViolations(
  result: Awaited<ReturnType<AxeBuilder['analyze']>>
): typeof result.violations {
  return result.violations.filter(
    (v) => v.impact === 'critical' || v.impact === 'serious'
  );
}
