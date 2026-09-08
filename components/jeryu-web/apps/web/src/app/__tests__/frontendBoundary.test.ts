import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const FORBIDDEN_IMPORTS = [
  'sqlx',
  'mysql',
  '@aws-sdk/client-s3',
  'pg',
  'better-sqlite3',
] as const;

function sourceFiles(root: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(root)) {
    const path = join(root, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      out.push(...sourceFiles(path));
    } else if (/\.(ts|tsx)$/.test(entry)) {
      out.push(path);
    }
  }
  return out;
}

function importedPackages(text: string): string[] {
  const imports: string[] = [];
  const importPattern = /(?:import|export)\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?['"]([^'"]+)['"]/g;
  for (const match of text.matchAll(importPattern)) {
    imports.push(match[1]);
  }
  return imports;
}

describe('frontend boundary', () => {
  it('does not import backend-only data clients', () => {
    const srcRoot = join(process.cwd(), 'src');
    const offenders: string[] = [];
    for (const file of sourceFiles(srcRoot)) {
      const imports = importedPackages(readFileSync(file, 'utf8'));
      for (const imported of imports) {
        if (FORBIDDEN_IMPORTS.some((name) => imported === name || imported.startsWith(`${name}/`))) {
          offenders.push(`${file}: ${imported}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
