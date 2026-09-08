#!/usr/bin/env node
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(scriptDir, '..');

const args = parseArgs(process.argv.slice(2));
const matrixPath = path.resolve(webRoot, args.matrix ?? 'e2e/action-matrix.json');
const junitPath = path.resolve(
  webRoot,
  args.junit ?? 'playwright-report/junit.xml'
);

const matrix = JSON.parse(await readText(matrixPath, 'action matrix'));
const actions = validateMatrix(matrix, matrixPath);
const expectedTags = new Map(
  actions.map((action) => [action.testTag, action])
);
const junit = await readText(junitPath, 'Playwright JUnit report');
const reportedTags = extractActionTags(junit);

const missing = actions.filter((action) => !reportedTags.has(action.testTag));
const undeclared = [...reportedTags].filter((tag) => !expectedTags.has(tag));

if (missing.length > 0 || undeclared.length > 0) {
  if (missing.length > 0) {
    console.error('Missing Playwright action coverage:');
    for (const action of missing) {
      console.error(`  ${action.testTag} ${action.route} ${action.action}`);
    }
  }
  if (undeclared.length > 0) {
    console.error('Playwright reported action tags not declared in the matrix:');
    for (const tag of undeclared) console.error(`  ${tag}`);
  }
  process.exit(1);
}

console.log(
  `Verified ${actions.length} action-matrix entries against ${reportedTags.size} reported Playwright action tags.`
);

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--matrix' || arg === '--junit') {
      const value = argv[i + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      parsed[arg.slice(2)] = value;
      i += 1;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return parsed;
}

async function readText(filePath, label) {
  try {
    return await readFile(filePath, 'utf8');
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`Could not read ${label} at ${filePath}: ${reason}`);
  }
}

function validateMatrix(matrix, sourcePath) {
  if (!matrix || typeof matrix !== 'object') {
    throw new Error(`${sourcePath} must contain a JSON object`);
  }
  if (!Array.isArray(matrix.actions)) {
    throw new Error(`${sourcePath} must contain an actions array`);
  }
  const seen = new Set();
  return matrix.actions.map((raw, index) => {
    if (!raw || typeof raw !== 'object') {
      throw new Error(`actions[${index}] must be an object`);
    }
    const action = raw;
    for (const key of ['id', 'route', 'surface', 'action', 'testTag', 'owner']) {
      if (typeof action[key] !== 'string' || action[key].trim() === '') {
        throw new Error(`actions[${index}].${key} must be a non-empty string`);
      }
    }
    if (!/^[a-z0-9][a-z0-9._-]*$/.test(action.id)) {
      throw new Error(`actions[${index}].id is not a stable action id: ${action.id}`);
    }
    const expectedTag = `@action:${action.id}`;
    if (action.testTag !== expectedTag) {
      throw new Error(
        `actions[${index}].testTag must be ${expectedTag}, got ${action.testTag}`
      );
    }
    if (seen.has(action.id)) {
      throw new Error(`Duplicate action id in matrix: ${action.id}`);
    }
    seen.add(action.id);
    if (
      action.variants !== undefined &&
      !Array.isArray(action.variants)
    ) {
      throw new Error(`actions[${index}].variants must be an array when present`);
    }
    return {
      id: action.id,
      route: action.route,
      surface: action.surface,
      action: action.action,
      testTag: action.testTag,
      owner: action.owner,
    };
  });
}

function extractActionTags(junitXml) {
  const tags = new Set();
  const testcasePattern = /<testcase\b[^>]*>/g;
  let match;
  while ((match = testcasePattern.exec(junitXml)) !== null) {
    const attrs = attributes(match[0]);
    const haystack = `${attrs.name ?? ''} ${attrs.classname ?? ''}`;
    for (const tag of haystack.match(/@action:[a-z0-9][a-z0-9._-]*/g) ?? []) {
      tags.add(tag);
    }
  }
  return tags;
}

function attributes(tag) {
  const attrs = {};
  const attrPattern = /([A-Za-z_:][-A-Za-z0-9_:.]*)="([^"]*)"/g;
  let match;
  while ((match = attrPattern.exec(tag)) !== null) {
    attrs[match[1]] = decodeXml(match[2]);
  }
  return attrs;
}

function decodeXml(value) {
  return value
    .replaceAll('&quot;', '"')
    .replaceAll('&apos;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&');
}
