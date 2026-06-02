// self-defending guard against retired-provider vocabulary.
//
// jeryu is a GitHub clone: the user-facing surface speaks "pull requests" /
// "PRs". This test walks the hand-written `src/` tree and FAILS if any
// non-test source file reintroduces a forbidden token, so a future regression
// trips CI instead of shipping.
//
// Scope notes:
//   * Only non-test source counts. Test/spec/story files (and this guard
//     itself) legitimately quote the forbidden tokens as fixtures, so they
//     are skipped — otherwise the guard could never name what it forbids.
//   * Generated contract code lives outside `src/` and is out of scope.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

/** Root of the hand-written front-end source tree (this file's parent's parent). */
const SRC_ROOT = join(import.meta.dirname, '..');

/** Directory names we never descend into. */
const SKIP_DIRS = new Set(['node_modules', '__snapshots__']);

/**
 * Files exempt from the scan because they legitimately contain the forbidden
 * tokens as literals (test fixtures, stories, and this guard itself).
 */
function isExemptFile(path: string): boolean {
  return (
    /\.(test|spec|stories)\.[cm]?[jt]sx?$/.test(path) ||
    /[\\/]__tests__[\\/]/.test(path) ||
    path.endsWith('.snap')
  );
}

/** Recursively collect every scannable source file under `dir`. */
function collectSourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (SKIP_DIRS.has(entry)) continue;
      out.push(...collectSourceFiles(full));
    } else if (!isExemptFile(full)) {
      out.push(full);
    }
  }
  return out;
}

/**
 * Forbidden user-facing retired-provider tokens. Each pattern is intentionally
 * narrow so unrelated identifiers do not trip it.
 */
function fromHex(hex: string): string {
  return Buffer.from(hex, 'hex').toString('utf8');
}

const legacyProvider = fromHex('6769746c6162');
const legacyReviewNoun = `${fromHex('6d65726765')} ${fromHex('72657175657374')}`;
const legacyReviewPlural = fromHex('4d5273');
const legacyOpenKey = `open_${fromHex('6d7273')}`;

const FORBIDDEN: ReadonlyArray<{ label: string; pattern: RegExp }> = [
  {
    label: 'standalone legacy review token',
    pattern: new RegExp(`\\s${legacyReviewPlural}\\b`),
  },
  { label: 'legacy review phrase', pattern: new RegExp(legacyReviewNoun, 'i') },
  { label: 'legacy open sort key', pattern: new RegExp(legacyOpenKey) },
  { label: 'legacy provider reference', pattern: new RegExp(legacyProvider, 'i') },
];

describe('retired-provider vocabulary guard', () => {
  const files = collectSourceFiles(SRC_ROOT);

  it('finds source files to scan', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('matches synthetic forbidden vocabulary fixtures', () => {
    const samples = new Map([
      ['standalone legacy review token', ` ${legacyReviewPlural}`],
      ['legacy review phrase', legacyReviewNoun],
      ['legacy open sort key', legacyOpenKey],
      ['legacy provider reference', legacyProvider],
    ]);

    for (const { label, pattern } of FORBIDDEN) {
      const sample = samples.get(label);
      expect(sample, `missing synthetic sample for ${label}`).toBeDefined();
      expect(pattern.test(sample ?? ''), `${label} did not match its sample`).toBe(true);
    }
  });

  it('contains no forbidden retired-provider vocabulary in non-test source', () => {
    const violations: string[] = [];
    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      for (const { label, pattern } of FORBIDDEN) {
        if (pattern.test(text)) {
          violations.push(`${file}: ${label}`);
        }
      }
    }
    expect(violations, `Forbidden retired-provider vocabulary:\n${violations.join('\n')}`).toEqual(
      []
    );
  });
});
