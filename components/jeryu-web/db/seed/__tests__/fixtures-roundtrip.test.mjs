// Round-trip proof for the dev/e2e fixture-seed store.
//
// Lives under db/ (NOT apps/web/src) so it can import better-sqlite3 without
// tripping apps/web/src/app/__tests__/frontendBoundary.test.ts (whose
// FORBIDDEN_IMPORTS includes 'better-sqlite3') or the direct-db-access /
// sql-bad-behavior caps. It has its own runner: `node --test`.
//
// Run:  node --test db/seed/__tests__/    (or `npm run fixtures:test`)

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { openSeededDb, deriveBootstrap } from '../build-fixtures.mjs';

const CANONICAL_BOOTSTRAP = JSON.parse(
  readFileSync(
    new URL('../../../apps/web/e2e/fixtures/data/bootstrap.json', import.meta.url),
    'utf8'
  )
);

function sortDeep(value) {
  if (Array.isArray(value)) return value.map(sortDeep);
  if (value && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = sortDeep(value[key]);
    return out;
  }
  return value;
}

test('sqlite_store_round_trips_fixtures', async (t) => {
  const db = openSeededDb();
  t.after(() => db.close());

  await t.test('seeds and reads back a viewer row', () => {
    const viewer = db
      .prepare('SELECT id, login, display_name FROM viewers WHERE id = ?')
      .get('usr_e2e');
    assert.equal(viewer.login, '@e2e');
    assert.equal(viewer.display_name, 'E2E Tester');
    const permCount = db
      .prepare('SELECT COUNT(*) AS n FROM viewer_permissions WHERE viewer_id = ?')
      .get('usr_e2e').n;
    assert.equal(permCount, 28);
  });

  await t.test('reads back a repository row and its owned pull request', () => {
    const repo = db.prepare('SELECT * FROM repositories WHERE id = ?').get('jeryu:neverhuman/jeryu');
    assert.equal(repo.visibility, 'internal');
    assert.equal(repo.jankurai_score, 92);
    const pr = db
      .prepare('SELECT * FROM pull_requests WHERE repo_id = ? AND number = ?')
      .get('jeryu:neverhuman/jeryu', 1);
    assert.equal(pr.state, 'open');
    assert.equal(pr.passport_hash, 'passport-hash-0001');
  });

  await t.test('NULL jankurai_score round-trips (unscoreable audit)', () => {
    const redline = db.prepare('SELECT jankurai_score, jankurai_decision FROM repositories WHERE id = ?').get('jeryu:veox/redline');
    assert.equal(redline.jankurai_score, null);
    assert.equal(redline.jankurai_decision, 'tool-failed');
  });

  await t.test('FK: a pull_request referencing an absent repo is rejected', () => {
    assert.throws(
      () =>
        db
          .prepare(
            `INSERT INTO pull_requests
               (id, repo_id, number, title, state, author, head_ref, base_ref, head_sha, base_sha, created_at, updated_at)
             VALUES ('pr_bad', 'jeryu:ghost/none', 99, 'x', 'open', '@a', 'h', 'b', 'aaa', 'bbb', 't', 't')`
          )
          .run(),
      /FOREIGN KEY constraint failed/
    );
  });

  await t.test('CHECK: an out-of-enum repository visibility is rejected', () => {
    assert.throws(
      () =>
        db
          .prepare(
            `INSERT INTO repositories (id, host, owner, name, default_branch, visibility, created_at, updated_at)
             VALUES ('r_bad', 'h', 'o', 'n', 'main', 'top-secret', 't', 't')`
          )
          .run(),
      /CHECK constraint failed/
    );
  });

  await t.test('CHECK: a merged pull_request without a passport_hash is rejected', () => {
    assert.throws(
      () =>
        db
          .prepare(
            `INSERT INTO pull_requests
               (id, repo_id, number, title, state, author, head_ref, base_ref, head_sha, base_sha, created_at, updated_at)
             VALUES ('pr_nohash', 'jeryu:neverhuman/jeryu', 42, 'x', 'merged', '@a', 'h', 'b', 'aaa', 'bbb', 't', 't')`
          )
          .run(),
      /CHECK constraint failed/
    );
  });

  await t.test('json_valid: a malformed labels_json is rejected', () => {
    assert.throws(
      () =>
        db
          .prepare(
            `INSERT INTO pull_requests
               (id, repo_id, number, title, state, author, head_ref, base_ref, head_sha, base_sha, labels_json, created_at, updated_at)
             VALUES ('pr_badjson', 'jeryu:neverhuman/jeryu', 43, 'x', 'open', '@a', 'h', 'b', 'aaa', 'bbb', '{not json', 't', 't')`
          )
          .run(),
      /CHECK constraint failed/
    );
  });

  await t.test('ON DELETE CASCADE prunes a repo\'s pulls, threads, and checks', () => {
    const scratch = openSeededDb();
    try {
      assert.ok(scratch.prepare('SELECT COUNT(*) AS n FROM pull_request_checks').get().n > 0);
      scratch.prepare('DELETE FROM repositories WHERE id = ?').run('jeryu:neverhuman/jeryu');
      assert.equal(scratch.prepare('SELECT COUNT(*) AS n FROM pull_requests').get().n, 0);
      assert.equal(scratch.prepare('SELECT COUNT(*) AS n FROM pull_request_threads').get().n, 0);
      assert.equal(scratch.prepare('SELECT COUNT(*) AS n FROM pull_request_checks').get().n, 0);
    } finally {
      scratch.close();
    }
  });

  await t.test('ON DELETE SET NULL detaches runner_tasks from a dropped repo', () => {
    const scratch = openSeededDb();
    try {
      const before = scratch.prepare('SELECT COUNT(*) AS n FROM runner_tasks').get().n;
      scratch.prepare('DELETE FROM repositories WHERE id = ?').run('jeryu:veox/redline');
      // The task survives (not cascaded) but its repo_id is detached to NULL.
      assert.equal(scratch.prepare('SELECT COUNT(*) AS n FROM runner_tasks').get().n, before);
      const detached = scratch
        .prepare("SELECT COUNT(*) AS n FROM runner_tasks WHERE id = 'task_2' AND repo_id IS NULL")
        .get().n;
      assert.equal(detached, 1);
    } finally {
      scratch.close();
    }
  });

  await t.test('derived bootstrap deep-equals the canonical e2e fixture', () => {
    const derived = deriveBootstrap(db);
    assert.deepEqual(sortDeep(derived), sortDeep(CANONICAL_BOOTSTRAP));
  });
});
