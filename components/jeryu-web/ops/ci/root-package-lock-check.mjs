#!/usr/bin/env node

import { lstatSync, readFileSync, realpathSync } from 'node:fs';
import { resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const DEPENDENCY_FIELDS = [
  'dependencies',
  'devDependencies',
  'optionalDependencies',
  'peerDependencies',
];

function readJson(path, label) {
  let text;
  try {
    text = readFileSync(path, 'utf8');
  } catch (error) {
    throw new Error(`${label} is unreadable: ${error.message}`);
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`${label} is not valid JSON: ${error.message}`);
  }
}

function requirePhysicalRoot(rootDir) {
  const root = resolve(rootDir);
  let metadata;
  let physical;
  try {
    metadata = lstatSync(root);
    physical = realpathSync(root);
  } catch (error) {
    throw new Error(`workspace root is unreadable: ${error.message}`);
  }
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || physical !== root) {
    throw new Error('workspace root must be a physical non-symlink directory');
  }
  return root;
}

function readContainedJson(root, relativePath, label) {
  const path = resolve(root, relativePath);
  if (!path.startsWith(`${root}${sep}`)) {
    throw new Error(`${label} must remain inside the workspace root`);
  }
  let metadata;
  let physical;
  try {
    metadata = lstatSync(path);
    physical = realpathSync(path);
  } catch (error) {
    throw new Error(`${label} is unreadable: ${error.message}`);
  }
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-symlink file`);
  }
  if (physical !== path) {
    throw new Error(`${label} must not traverse symlinked path components`);
  }
  return readJson(path, label);
}

function requireRecord(value, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requireStringArray(value, label) {
  if (!Array.isArray(value) || value.some((item) => typeof item !== 'string')) {
    throw new Error(`${label} must be an array of strings`);
  }
  if (new Set(value).size !== value.length) {
    throw new Error(`${label} must not contain duplicates`);
  }
  return value;
}

function sortedStrings(values) {
  return [...values].sort((left, right) => left.localeCompare(right));
}

function requireDependencyMap(value, label) {
  if (value === undefined) return {};
  const record = requireRecord(value, label);
  for (const [name, requirement] of Object.entries(record)) {
    if (!name || typeof requirement !== 'string' || !requirement) {
      throw new Error(`${label} must map non-empty names to non-empty strings`);
    }
  }
  return record;
}

function dependencyEntries(value, label) {
  return Object.entries(requireDependencyMap(value, label)).sort(([left], [right]) =>
    left.localeCompare(right)
  );
}

function requireLiteralWorkspace(workspace) {
  if (
    !workspace ||
    workspace.startsWith('/') ||
    workspace.includes('\\') ||
    /[\u0000-\u001F\u007F]/u.test(workspace) ||
    workspace.split('/').some((part) => !part || part === '.' || part === '..') ||
    /[*?[\]{}!]/u.test(workspace)
  ) {
    throw new Error(`package.json workspace must be a literal relative path: ${workspace}`);
  }
}

export function validateRootPackageLock(packageJson, packageLock, workspacePackages) {
  const manifest = requireRecord(packageJson, 'package.json');
  const lock = requireRecord(packageLock, 'package-lock.json');
  if (lock.lockfileVersion !== 3) {
    throw new Error('package-lock.json lockfileVersion must be exactly 3');
  }

  const packages = requireRecord(lock.packages, 'package-lock.json packages');
  const root = requireRecord(packages[''], 'package-lock.json root package record');
  if (lock.name !== manifest.name) {
    throw new Error('package-lock.json top-level name must equal package.json name');
  }
  if (typeof manifest.name !== 'string' || !manifest.name || root.name !== manifest.name) {
    throw new Error('package-lock.json root name must equal package.json name');
  }

  const manifestWorkspaces = requireStringArray(
    manifest.workspaces,
    'package.json workspaces'
  );
  const lockWorkspaces = requireStringArray(
    root.workspaces,
    'package-lock.json root workspaces'
  );
  if (
    JSON.stringify(sortedStrings(manifestWorkspaces)) !==
    JSON.stringify(sortedStrings(lockWorkspaces))
  ) {
    throw new Error('package-lock.json root workspaces must equal package.json workspaces');
  }
  const localWorkspaceEntries = Object.keys(packages).filter(
    (path) => path && !path.split('/').includes('node_modules')
  );
  if (
    JSON.stringify(sortedStrings(localWorkspaceEntries)) !==
    JSON.stringify(sortedStrings(manifestWorkspaces))
  ) {
    throw new Error('package-lock.json local workspace inventory must equal package.json workspaces');
  }

  for (const field of DEPENDENCY_FIELDS) {
    const manifestEntries = dependencyEntries(
      manifest[field],
      `package.json ${field}`
    );
    const lockEntries = dependencyEntries(
      root[field],
      `package-lock.json root ${field}`
    );
    if (JSON.stringify(manifestEntries) !== JSON.stringify(lockEntries)) {
      throw new Error(`package-lock.json root ${field} must equal package.json ${field}`);
    }
  }

  const workspaceRecords = requireRecord(workspacePackages, 'workspace package records');
  for (const workspace of manifestWorkspaces) {
    requireLiteralWorkspace(workspace);
    const workspaceManifest = requireRecord(
      workspaceRecords[workspace],
      `${workspace}/package.json`
    );
    const lockWorkspace = requireRecord(
      packages[workspace],
      `package-lock.json workspace record ${workspace}`
    );
    if (
      typeof workspaceManifest.name !== 'string' ||
      !workspaceManifest.name ||
      lockWorkspace.name !== workspaceManifest.name
    ) {
      throw new Error(
        `package-lock.json workspace ${workspace} name must equal its package.json name`
      );
    }
  }
}

export function checkRootPackageLock(rootDir) {
  const root = requirePhysicalRoot(rootDir);
  const packageJson = readContainedJson(root, 'package.json', 'package.json');
  const packageLock = readContainedJson(root, 'package-lock.json', 'package-lock.json');
  const workspaces = requireStringArray(
    requireRecord(packageJson, 'package.json').workspaces,
    'package.json workspaces'
  );
  for (const workspace of workspaces) {
    requireLiteralWorkspace(workspace);
  }
  const workspacePackages = Object.fromEntries(
    workspaces.map((workspace) => [
      workspace,
      readContainedJson(root, `${workspace}/package.json`, `${workspace}/package.json`),
    ])
  );
  validateRootPackageLock(packageJson, packageLock, workspacePackages);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : '';
if (invokedPath === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 2) {
    console.error('root-package-lock-check accepts no arguments');
    process.exit(2);
  }
  try {
    checkRootPackageLock(process.cwd());
    console.log('root package-lock ok');
  } catch (error) {
    console.error(`root package-lock check failed: ${error.message}`);
    process.exit(1);
  }
}
