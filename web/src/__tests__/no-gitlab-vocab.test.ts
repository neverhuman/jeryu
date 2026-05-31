// no-gitlab-vocab.test.ts — self-defending guard against GitLab vocabulary.
//
// jeryu is a GitHub clone: the user-facing surface speaks "pull requests" /
// "PRs", never GitLab's "merge requests" / "MRs". This test walks the
// hand-written `src/` tree and FAILS if any non-test source file reintroduces
// a forbidden token, so a future regression trips CI instead of shipping.
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
 * Forbidden user-facing GitLab tokens. Each pattern is intentionally narrow so
 * unrelated identifiers (e.g. an English word ending in "mrs") do not trip it.
 */
const FORBIDDEN: ReadonlyArray<{ label: string; pattern: RegExp }> = [
  { label: 'standalone "MRs" token', pattern: /\sMRs\b/ },
  { label: '"merge request" phrase', pattern: /merge request/i },
  { label: 'open_mrs sort key', pattern: /open_mrs/ },
  { label: '"gitlab" reference', pattern: /gitlab/i },
];

describe('GitLab vocabulary guard', () => {
  const files = collectSourceFiles(SRC_ROOT);

  it('finds source files to scan', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it('contains no forbidden GitLab vocabulary in non-test source', () => {
    const violations: string[] = [];
    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      for (const { label, pattern } of FORBIDDEN) {
        if (pattern.test(text)) {
          violations.push(`${file}: ${label}`);
        }
      }
    }
    expect(violations, `Forbidden GitLab vocabulary:\n${violations.join('\n')}`).toEqual([]);
  });
});
