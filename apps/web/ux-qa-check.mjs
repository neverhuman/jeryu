import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const workspaceDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(workspaceDir, '..', '..');
const mode = process.argv[2] ?? 'build';

if (!['build', 'test'].includes(mode)) {
  console.error('usage: node ux-qa-check.mjs <build|test>');
  process.exit(2);
}

const evidenceFiles = [
  ['ux-qa.ts', join(workspaceDir, 'ux-qa.ts')],
  ['ux-qa.md', join(workspaceDir, 'ux-qa.md')],
];

const ts = readFileSync(evidenceFiles[0][1], 'utf8');
const md = readFileSync(evidenceFiles[1][1], 'utf8');

const requiredTsSnippets = [
  'storybook',
  'playwright_visual',
  'visual_review',
  'accessibility',
  'layout_stability',
  'api_mocks',
  'design_tokens',
  'artifact_backed_proof',
];
const requiredMdSnippets = [
  '# Rendered UX QA Evidence',
  'Storybook state coverage',
  '@jankurai/ux-qa',
  'Artifact-backed proof receipts',
];

const missing = [
  ...requiredTsSnippets
    .filter((snippet) => !ts.includes(snippet))
    .map((snippet) => `ux-qa.ts:${snippet}`),
  ...requiredMdSnippets
    .filter((snippet) => !md.includes(snippet))
    .map((snippet) => `ux-qa.md:${snippet}`),
];

if (missing.length > 0) {
  console.error(`ux-qa ${mode} failed; missing evidence markers: ${missing.join(', ')}`);
  process.exit(1);
}

const digest = (value) => createHash('sha256').update(value).digest('hex');
const artifactDir = join(repoRoot, 'target', 'jankurai', 'ux-qa', 'npm-workspace');
mkdirSync(artifactDir, { recursive: true });
writeFileSync(
  join(artifactDir, `${mode}.json`),
  JSON.stringify(
    {
      mode,
      workspace: '@jankurai/ux-qa',
      files: evidenceFiles.map(([name, path]) => ({
        name,
        path,
        sha256: digest(readFileSync(path, 'utf8')),
      })),
    },
    null,
    2,
  ) + '\n',
);

console.log(`@jankurai/ux-qa ${mode} ok`);
