import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { checkRootPackageLock } from './root-package-lock-check.mjs';

const roots = [];

function fixture(mutator = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'jeryu-web-root-lock-'));
  roots.push(root);
  const packageJson = {
    name: 'fixture-root',
    private: true,
    workspaces: ['apps/web', 'ux-qa'],
    devDependencies: { 'fixture-db': '1.2.3' },
  };
  const packageLock = {
    name: 'fixture-root',
    lockfileVersion: 3,
    packages: {
      '': {
        name: 'fixture-root',
        workspaces: ['apps/web', 'ux-qa'],
        devDependencies: { 'fixture-db': '1.2.3' },
      },
      'apps/web': { name: '@fixture/web' },
      'ux-qa': { name: '@fixture/ux-qa' },
    },
  };
  const workspacePackages = {
    'apps/web': { name: '@fixture/web' },
    'ux-qa': { name: '@fixture/ux-qa' },
  };
  mutator({ packageJson, packageLock, workspacePackages, root });
  mkdirSync(join(root, 'apps/web'), { recursive: true });
  mkdirSync(join(root, 'ux-qa'), { recursive: true });
  writeFileSync(join(root, 'package.json'), `${JSON.stringify(packageJson)}\n`);
  writeFileSync(join(root, 'package-lock.json'), `${JSON.stringify(packageLock)}\n`);
  for (const [workspace, manifest] of Object.entries(workspacePackages)) {
    writeFileSync(join(root, workspace, 'package.json'), `${JSON.stringify(manifest)}\n`);
  }
  return root;
}

function hostileWorkspaceFixture(workspace) {
  const root = mkdtempSync(join(tmpdir(), 'jeryu-web-hostile-workspace-'));
  roots.push(root);
  const packageJson = {
    name: 'fixture-root',
    private: true,
    workspaces: [workspace],
  };
  const packageLock = {
    name: 'fixture-root',
    lockfileVersion: 3,
    packages: {
      '': {
        name: 'fixture-root',
        workspaces: [workspace],
      },
    },
  };
  writeFileSync(join(root, 'package.json'), `${JSON.stringify(packageJson)}\n`);
  writeFileSync(join(root, 'package-lock.json'), `${JSON.stringify(packageLock)}\n`);
  return root;
}

test.afterEach(() => {
  while (roots.length > 0) {
    rmSync(roots.pop(), { recursive: true, force: true });
  }
});

test('accepts matching root and workspace lock metadata', () => {
  assert.doesNotThrow(() => checkRootPackageLock(fixture()));
});

test('rejects malformed package-lock JSON', () => {
  const root = fixture();
  writeFileSync(join(root, 'package-lock.json'), '{');
  assert.throws(() => checkRootPackageLock(root), /package-lock\.json is not valid JSON/u);
});

test('rejects a missing root package record', () => {
  const root = fixture(({ packageLock }) => delete packageLock.packages['']);
  assert.throws(() => checkRootPackageLock(root), /root package record must be an object/u);
});

test('rejects a root name mismatch', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.packages[''].name = 'wrong-root';
  });
  assert.throws(() => checkRootPackageLock(root), /root name must equal/u);
});

test('rejects a top-level lock name mismatch', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.name = 'wrong-root';
  });
  assert.throws(() => checkRootPackageLock(root), /top-level name must equal/u);
});

test('rejects a workspace list mismatch', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.packages[''].workspaces = ['apps/web'];
  });
  assert.throws(() => checkRootPackageLock(root), /root workspaces must equal/u);
});

test('rejects a root dependency mismatch', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.packages[''].devDependencies['fixture-db'] = '9.9.9';
  });
  assert.throws(() => checkRootPackageLock(root), /root devDependencies must equal/u);
});

test('rejects a missing workspace lock entry', () => {
  const root = fixture(({ packageLock }) => delete packageLock.packages['ux-qa']);
  assert.throws(() => checkRootPackageLock(root), /local workspace inventory must equal/u);
});

test('rejects an extra stale local workspace lock entry', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.packages['stale-workspace'] = { name: '@fixture/stale' };
  });
  assert.throws(() => checkRootPackageLock(root), /local workspace inventory must equal/u);
});

test('rejects a workspace package name mismatch', () => {
  const root = fixture(({ packageLock }) => {
    packageLock.packages['apps/web'].name = '@fixture/wrong';
  });
  assert.throws(() => checkRootPackageLock(root), /workspace apps\/web name must equal/u);
});

test('rejects a symlinked workspace path before reading its manifest', () => {
  const root = fixture();
  const outside = mkdtempSync(join(tmpdir(), 'jeryu-web-outside-workspace-'));
  roots.push(outside);
  writeFileSync(join(outside, 'package.json'), '{');
  rmSync(join(root, 'apps/web'), { recursive: true, force: true });
  symlinkSync(outside, join(root, 'apps/web'), 'dir');
  assert.throws(
    () => checkRootPackageLock(root),
    /must not traverse symlinked path components/u
  );
});

for (const [label, workspace] of [
  ['absolute', '/tmp/jeryu-web-outside-workspace'],
  ['traversal', '../jeryu-web-outside-workspace'],
  ['glob', 'apps/*'],
  ['control-character', 'apps/\nweb'],
]) {
  test(`rejects ${label} workspace paths before reading them`, () => {
    const root = hostileWorkspaceFixture(workspace);
    assert.throws(
      () => checkRootPackageLock(root),
      /workspace must be a literal relative path/u
    );
  });
}
