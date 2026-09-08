// build-fixtures.mjs — dev/e2e fixture-seed store builder.
//
// This is the ONLY place that opens the fixture SQLite store. It lives under
// db/ (never apps/web/src) so `better-sqlite3` and raw SQL never leak into the
// SPA bundle or trip the frontend boundary test
// (apps/web/src/app/__tests__/frontendBoundary.test.ts).
//
// What it does, self-contained:
//   1. Opens an ephemeral (in-memory by default) SQLite database.
//   2. Applies every db/migrations/*.sql then every db/constraints/*.sql in
//      lexical order — the exact files migration analysis audits.
//   3. Inserts deterministic seed rows (fixed ids + timestamps) that satisfy
//      the CHECK / FOREIGN KEY / json_valid constraints.
//   4. Derives and writes the fixture JSON the test layer consumes into
//      db/fixtures/*.json (a declared generated zone). The emitted
//      db/fixtures/bootstrap.json reproduces the canonical e2e bootstrap
//      fixture (apps/web/e2e/fixtures/data/bootstrap.json) from the store, so
//      the store is provably the source of truth for it.
//
// Run:  node db/seed/build-fixtures.mjs   (or `npm run fixtures:build`)

import Database from 'better-sqlite3';
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const MIGRATIONS_DIR = new URL('../migrations/', import.meta.url);
const CONSTRAINTS_DIR = new URL('../constraints/', import.meta.url);
const DEFAULT_OUT_DIR = new URL('../fixtures/', import.meta.url);

// Fixture-envelope constants that are not relational truth (they wrap the
// viewer/permission/flag rows the store owns). Kept here so the emitted
// bootstrap matches the canonical e2e fixture byte-for-byte.
const GENERATED_AT = '2026-05-27T00:00:00Z';
const SCHEMA_VERSION = '0.1.0-alpha';
const WEBSOCKET_URL = '/api/v1/ws';
const SEED_TS = '2026-05-26T00:00:00Z';
// Canonical feature-flag order (bootstrap.json is not alphabetical).
const FLAG_ORDER = [
  'repo_create',
  'settings_write',
  'merge_write',
  'markdown_html',
  'agents',
  'mcp',
  'workcells',
];

// --------------------------------------------------------------------------
// Deterministic seed data
// --------------------------------------------------------------------------

const VIEWER = {
  id: 'usr_e2e',
  login: '@e2e',
  display_name: 'E2E Tester',
  avatar_url: null,
};

const PERMISSIONS = [
  'admin.audit', 'agents.grant', 'agents.read', 'agents.write', 'audit.read',
  'branch.create', 'branch.delete', 'ci.read', 'ci.write', 'code.read',
  'code.write', 'issue.read', 'issue.write', 'pr.approve', 'pr.comment',
  'pr.merge', 'pr.read', 'pr.review', 'pr.write', 'repo.admin', 'repo.create',
  'repo.delete', 'repo.read', 'repo.write', 'secrets.read_metadata',
  'secrets.write', 'settings.read', 'settings.write',
];

const FEATURE_FLAGS = {
  repo_create: false,
  settings_write: false,
  merge_write: false,
  markdown_html: true,
  agents: false,
  mcp: false,
  workcells: false,
};

const REPOSITORIES = [
  {
    id: 'jeryu:neverhuman/jeryu',
    host: 'jeryu', owner: 'neverhuman', name: 'jeryu',
    default_branch: 'main',
    description: 'The Jeryu forge core.',
    visibility: 'internal',
    family: 'neverhuman',
    repo_role: 'split_member',
    health: 'green',
    topics: ['forge', 'rust'],
    open_pull_requests: 2,
    failing_checks: 0,
    running_jobs: 1,
    active_agents: 0,
    jankurai_score: 92,
    jankurai_decision: 'pass',
    jankurai_scored_at: SEED_TS,
  },
  {
    id: 'jeryu:veox/redline',
    host: 'jeryu', owner: 'veox', name: 'redline',
    default_branch: 'main',
    description: 'Fleet control surface (non-Rust; unscoreable audit).',
    visibility: 'private',
    family: 'redline',
    repo_role: 'split_member',
    health: 'yellow',
    topics: ['fleet'],
    open_pull_requests: 0,
    failing_checks: 1,
    running_jobs: 2,
    active_agents: 1,
    // decision='tool-failed' records an unscoreable audit — NULL score, matching
    // jeryu-core 0007_jankurai_scores semantics.
    jankurai_score: null,
    jankurai_decision: 'tool-failed',
    jankurai_scored_at: SEED_TS,
  },
];

// Pull requests (with owned threads + checks) for the primary repo.
const PULL_REQUESTS = [
  {
    id: 'pr_jeryu_1',
    repo_id: 'jeryu:neverhuman/jeryu',
    number: 1,
    title: 'Add fixture-seed store',
    state: 'open',
    draft: 0,
    author: '@author',
    head_ref: 'feature/fixture-store',
    base_ref: 'main',
    head_sha: 'a'.repeat(40),
    base_sha: 'base000000000000000000000000000000000000',
    can_merge: 0,
    passport_status: 'blocked',
    passport_hash: 'passport-hash-0001',
    required_approvals: 1,
    approvals: 0,
    changes_requested: 0,
    unresolved_threads: 1,
    labels: ['infra'],
    threads: [
      {
        id: 'th_1_1', author: '@reviewer', path: 'db/migrations/0001_fixture_model.sql',
        line: 12, body: 'Confirm the cascade order matches the rollback.', resolved: 0,
      },
    ],
    checks: [
      { id: 'ck_1_1', name: 'build', status: 'completed', conclusion: 'success' },
      { id: 'ck_1_2', name: 'test', status: 'completed', conclusion: 'success' },
    ],
  },
  {
    id: 'pr_jeryu_7',
    repo_id: 'jeryu:neverhuman/jeryu',
    number: 7,
    title: 'Wire fleet rollups',
    state: 'merged',
    draft: 0,
    author: '@author',
    head_ref: 'feature/fleet-rollups',
    base_ref: 'main',
    head_sha: 'c'.repeat(40),
    base_sha: 'base000000000000000000000000000000000000',
    can_merge: 1,
    passport_status: 'pass',
    passport_hash: 'passport-hash-0007',
    required_approvals: 1,
    approvals: 2,
    changes_requested: 0,
    unresolved_threads: 0,
    labels: [],
    threads: [],
    checks: [
      { id: 'ck_7_1', name: 'build', status: 'completed', conclusion: 'success' },
      { id: 'ck_7_2', name: 'test', status: 'completed', conclusion: 'success' },
    ],
  },
];

// Runner pools -> runners -> tasks (fleet pool_activity).
const RUNNER_POOLS = [
  {
    id: 'pool_default', name: 'default', trust_tier: 'trusted', paused: 0,
    configured_max_slots: 4, tags: ['linux', 'x64'],
    runners: [
      { id: 'run_default_1', name: 'default-1', status: 'online', active_slots: 1,
        tasks: [
          { id: 'task_1', repo_id: 'jeryu:neverhuman/jeryu', kind: 'build',
            state: 'running', attempt: 1, payload: { pr: 1 } },
        ],
      },
      { id: 'run_default_2', name: 'default-2', status: 'idle', active_slots: 0, tasks: [] },
    ],
  },
  {
    id: 'pool_gpu', name: 'gpu', trust_tier: 'partner', paused: 0,
    configured_max_slots: 2, tags: ['linux', 'gpu'],
    runners: [
      { id: 'run_gpu_1', name: 'gpu-1', status: 'busy', active_slots: 2,
        tasks: [
          { id: 'task_2', repo_id: 'jeryu:veox/redline', kind: 'test',
            state: 'running', attempt: 1, payload: {} },
          { id: 'task_3', repo_id: null, kind: 'agent',
            state: 'queued', attempt: 1, payload: { agent: 'editbot' } },
        ],
      },
    ],
  },
];

const WORK_ITEMS = [
  {
    id: 'wi_1', viewer_id: 'usr_e2e', repo_id: 'jeryu:neverhuman/jeryu',
    kind: 'pull_request', title: 'Review PR #1', state: 'todo', priority: 'high',
    url: '/jeryu/neverhuman/jeryu/pulls/1', metadata: { pr: 1 },
  },
  {
    id: 'wi_2', viewer_id: 'usr_e2e', repo_id: 'jeryu:veox/redline',
    kind: 'check', title: 'Investigate failing check on redline', state: 'in_progress',
    priority: 'urgent', url: '/jeryu/veox/redline', metadata: {},
  },
  {
    id: 'wi_3', viewer_id: 'usr_e2e', repo_id: null,
    kind: 'agent_run', title: 'Approve agent budget', state: 'blocked',
    priority: 'low', url: null, metadata: { reason: 'awaiting_admin' },
  },
];

// --------------------------------------------------------------------------
// Store construction
// --------------------------------------------------------------------------

function applySqlDir(db, dirUrl) {
  const dir = fileURLToPath(dirUrl);
  const files = readdirSync(dir)
    .filter((f) => f.endsWith('.sql'))
    .sort();
  for (const file of files) {
    db.exec(readFileSync(new URL(file, dirUrl), 'utf8'));
  }
  return files;
}

/**
 * Open an in-memory SQLite store, apply the real migrations + constraints, and
 * insert the deterministic seed rows. Returns the open Database (caller closes).
 */
export function openSeededDb() {
  const db = new Database(':memory:');
  db.pragma('foreign_keys = ON');
  const migrations = applySqlDir(db, MIGRATIONS_DIR);
  applySqlDir(db, CONSTRAINTS_DIR);
  db.__migrations = migrations;

  const insertViewer = db.prepare(
    'INSERT INTO viewers (id, login, display_name, avatar_url, created_at) VALUES (?, ?, ?, ?, ?)'
  );
  const insertPerm = db.prepare(
    'INSERT INTO viewer_permissions (viewer_id, permission) VALUES (?, ?)'
  );
  const insertFlag = db.prepare(
    'INSERT INTO feature_flags (viewer_id, flag, enabled) VALUES (?, ?, ?)'
  );
  const insertRepo = db.prepare(
    `INSERT INTO repositories
       (id, host, owner, name, default_branch, description, visibility, family,
        repo_role, health, topics_json, open_pull_requests, failing_checks,
        running_jobs, active_agents, jankurai_score, jankurai_decision,
        jankurai_scored_at, created_at, updated_at)
     VALUES
       (@id, @host, @owner, @name, @default_branch, @description, @visibility,
        @family, @repo_role, @health, @topics_json, @open_pull_requests,
        @failing_checks, @running_jobs, @active_agents, @jankurai_score,
        @jankurai_decision, @jankurai_scored_at, @created_at, @updated_at)`
  );
  const insertPull = db.prepare(
    `INSERT INTO pull_requests
       (id, repo_id, number, title, state, draft, author, head_ref, base_ref,
        head_sha, base_sha, can_merge, passport_status, passport_hash,
        required_approvals, approvals, changes_requested, unresolved_threads,
        labels_json, created_at, updated_at)
     VALUES
       (@id, @repo_id, @number, @title, @state, @draft, @author, @head_ref,
        @base_ref, @head_sha, @base_sha, @can_merge, @passport_status,
        @passport_hash, @required_approvals, @approvals, @changes_requested,
        @unresolved_threads, @labels_json, @created_at, @updated_at)`
  );
  const insertThread = db.prepare(
    `INSERT INTO pull_request_threads
       (id, pull_request_id, author, path, line, body, resolved, created_at)
     VALUES (@id, @pull_request_id, @author, @path, @line, @body, @resolved, @created_at)`
  );
  const insertCheck = db.prepare(
    `INSERT INTO pull_request_checks
       (id, pull_request_id, name, status, conclusion, started_at, completed_at)
     VALUES (@id, @pull_request_id, @name, @status, @conclusion, @started_at, @completed_at)`
  );
  const insertPool = db.prepare(
    `INSERT INTO runner_pools
       (id, name, trust_tier, paused, configured_max_slots, tags_json, created_at)
     VALUES (@id, @name, @trust_tier, @paused, @configured_max_slots, @tags_json, @created_at)`
  );
  const insertRunner = db.prepare(
    `INSERT INTO runners (id, pool_id, name, status, active_slots, created_at)
     VALUES (@id, @pool_id, @name, @status, @active_slots, @created_at)`
  );
  const insertTask = db.prepare(
    `INSERT INTO runner_tasks
       (id, runner_id, repo_id, kind, state, attempt, payload_json, created_at, updated_at)
     VALUES (@id, @runner_id, @repo_id, @kind, @state, @attempt, @payload_json, @created_at, @updated_at)`
  );
  const insertWork = db.prepare(
    `INSERT INTO work_items
       (id, viewer_id, repo_id, kind, title, state, priority, url, metadata_json, created_at, updated_at)
     VALUES (@id, @viewer_id, @repo_id, @kind, @title, @state, @priority, @url, @metadata_json, @created_at, @updated_at)`
  );

  const seed = db.transaction(() => {
    insertViewer.run(VIEWER.id, VIEWER.login, VIEWER.display_name, VIEWER.avatar_url, SEED_TS);
    for (const p of PERMISSIONS) insertPerm.run(VIEWER.id, p);
    for (const flag of FLAG_ORDER) insertFlag.run(VIEWER.id, flag, FEATURE_FLAGS[flag] ? 1 : 0);

    for (const r of REPOSITORIES) {
      insertRepo.run({
        id: r.id, host: r.host, owner: r.owner, name: r.name,
        default_branch: r.default_branch, description: r.description,
        visibility: r.visibility, family: r.family, repo_role: r.repo_role,
        health: r.health, topics_json: JSON.stringify(r.topics),
        open_pull_requests: r.open_pull_requests, failing_checks: r.failing_checks,
        running_jobs: r.running_jobs, active_agents: r.active_agents,
        jankurai_score: r.jankurai_score, jankurai_decision: r.jankurai_decision,
        jankurai_scored_at: r.jankurai_scored_at, created_at: SEED_TS, updated_at: SEED_TS,
      });
    }

    for (const pr of PULL_REQUESTS) {
      insertPull.run({
        id: pr.id, repo_id: pr.repo_id, number: pr.number, title: pr.title,
        state: pr.state, draft: pr.draft, author: pr.author, head_ref: pr.head_ref,
        base_ref: pr.base_ref, head_sha: pr.head_sha, base_sha: pr.base_sha,
        can_merge: pr.can_merge, passport_status: pr.passport_status,
        passport_hash: pr.passport_hash, required_approvals: pr.required_approvals,
        approvals: pr.approvals, changes_requested: pr.changes_requested,
        unresolved_threads: pr.unresolved_threads, labels_json: JSON.stringify(pr.labels),
        created_at: SEED_TS, updated_at: SEED_TS,
      });
      for (const th of pr.threads) {
        insertThread.run({
          id: th.id, pull_request_id: pr.id, author: th.author, path: th.path,
          line: th.line, body: th.body, resolved: th.resolved, created_at: SEED_TS,
        });
      }
      for (const ck of pr.checks) {
        insertCheck.run({
          id: ck.id, pull_request_id: pr.id, name: ck.name, status: ck.status,
          conclusion: ck.conclusion, started_at: SEED_TS,
          completed_at: ck.status === 'completed' ? SEED_TS : null,
        });
      }
    }

    for (const pool of RUNNER_POOLS) {
      insertPool.run({
        id: pool.id, name: pool.name, trust_tier: pool.trust_tier, paused: pool.paused,
        configured_max_slots: pool.configured_max_slots,
        tags_json: JSON.stringify(pool.tags), created_at: SEED_TS,
      });
      for (const runner of pool.runners) {
        insertRunner.run({
          id: runner.id, pool_id: pool.id, name: runner.name, status: runner.status,
          active_slots: runner.active_slots, created_at: SEED_TS,
        });
        for (const task of runner.tasks) {
          insertTask.run({
            id: task.id, runner_id: runner.id, repo_id: task.repo_id, kind: task.kind,
            state: task.state, attempt: task.attempt,
            payload_json: JSON.stringify(task.payload), created_at: SEED_TS, updated_at: SEED_TS,
          });
        }
      }
    }

    for (const wi of WORK_ITEMS) {
      insertWork.run({
        id: wi.id, viewer_id: wi.viewer_id, repo_id: wi.repo_id, kind: wi.kind,
        title: wi.title, state: wi.state, priority: wi.priority, url: wi.url,
        metadata_json: JSON.stringify(wi.metadata), created_at: SEED_TS, updated_at: SEED_TS,
      });
    }
  });
  seed();

  return db;
}

// --------------------------------------------------------------------------
// Derivation: store rows -> fixture JSON the test layer consumes
// --------------------------------------------------------------------------

/** Reconstruct the canonical bootstrap fixture for a viewer from the store. */
export function deriveBootstrap(db, viewerId = VIEWER.id) {
  const viewer = db
    .prepare('SELECT id, login, display_name, avatar_url FROM viewers WHERE id = ?')
    .get(viewerId);
  const permissions = db
    .prepare('SELECT permission FROM viewer_permissions WHERE viewer_id = ? ORDER BY permission')
    .all(viewerId)
    .map((row) => row.permission);
  const flagRows = db
    .prepare('SELECT flag, enabled FROM feature_flags WHERE viewer_id = ?')
    .all(viewerId);
  const flagMap = new Map(flagRows.map((r) => [r.flag, r.enabled === 1]));
  const feature_flags = {};
  for (const flag of FLAG_ORDER) {
    if (flagMap.has(flag)) feature_flags[flag] = flagMap.get(flag);
  }
  const recent = db
    .prepare(
      `SELECT r.host, r.owner, r.name FROM recent_repositories rr
         JOIN repositories r ON r.id = rr.repo_id
        WHERE rr.viewer_id = ? ORDER BY rr.position`
    )
    .all(viewerId);

  return {
    generated_at: GENERATED_AT,
    schema_version: SCHEMA_VERSION,
    viewer: {
      id: viewer.id,
      login: viewer.login,
      display_name: viewer.display_name,
      avatar_url: viewer.avatar_url,
      global_permissions: permissions,
    },
    tui: {},
    recent_repositories: recent,
    websocket_url: WEBSOCKET_URL,
    feature_flags,
  };
}

/** Repositories in the e2e `MockRepoSummary` shape (input to mockRepoList). */
export function deriveRepositories(db) {
  return db
    .prepare('SELECT * FROM repositories ORDER BY id')
    .all()
    .map((r) => ({
      id: { host: r.host, owner: r.owner, name: r.name },
      default_branch: r.default_branch,
      description: r.description,
      visibility: r.visibility,
      family: r.family,
      repo_role: r.repo_role,
      topics: JSON.parse(r.topics_json),
      open_pull_requests: r.open_pull_requests,
      failing_checks: r.failing_checks,
      running_jobs: r.running_jobs,
      active_agents: r.active_agents,
      jankurai_score: r.jankurai_score,
      jankurai_decision: r.jankurai_decision,
      jankurai_scored_at: r.jankurai_scored_at,
    }));
}

export function derivePullRequests(db) {
  const pulls = db.prepare('SELECT * FROM pull_requests ORDER BY repo_id, number').all();
  const threadStmt = db.prepare(
    'SELECT author, path, line, body, resolved FROM pull_request_threads WHERE pull_request_id = ? ORDER BY id'
  );
  const checkStmt = db.prepare(
    'SELECT name, status, conclusion FROM pull_request_checks WHERE pull_request_id = ? ORDER BY name'
  );
  return pulls.map((p) => ({
    repo_id: p.repo_id,
    number: p.number,
    title: p.title,
    state: p.state,
    draft: p.draft === 1,
    author: p.author,
    head_ref: p.head_ref,
    base_ref: p.base_ref,
    head_sha: p.head_sha,
    base_sha: p.base_sha,
    can_merge: p.can_merge === 1,
    passport_status: p.passport_status,
    passport_hash: p.passport_hash,
    review: {
      required_approvals: p.required_approvals,
      approvals: p.approvals,
      changes_requested: p.changes_requested,
      unresolved_threads: p.unresolved_threads,
    },
    labels: JSON.parse(p.labels_json),
    threads: threadStmt.all(p.id).map((t) => ({ ...t, resolved: t.resolved === 1 })),
    checks: checkStmt.all(p.id),
  }));
}

/** Runner pools in the e2e `MockPoolRollup` shape (input to mockFleetBootstrap). */
export function deriveFleet(db) {
  const pools = db.prepare('SELECT * FROM runner_pools ORDER BY name').all();
  const runnersStmt = db.prepare('SELECT * FROM runners WHERE pool_id = ?');
  const taskCountStmt = db.prepare(
    `SELECT rt.state AS state, COUNT(*) AS n FROM runner_tasks rt
       JOIN runners r ON r.id = rt.runner_id
      WHERE r.pool_id = ? GROUP BY rt.state`
  );
  return pools.map((pool) => {
    const runners = runnersStmt.all(pool.id);
    const counts = Object.fromEntries(taskCountStmt.all(pool.id).map((r) => [r.state, r.n]));
    return {
      pool: pool.name,
      tags: JSON.parse(pool.tags_json),
      trust_tier: pool.trust_tier,
      paused: pool.paused === 1,
      queued_jobs: counts.queued ?? 0,
      running_jobs: counts.running ?? 0,
      failed_jobs: counts.failed ?? 0,
      active_slots: runners.reduce((sum, r) => sum + r.active_slots, 0),
      configured_max_slots: pool.configured_max_slots,
      online_runners: runners.filter((r) => ['online', 'busy', 'idle'].includes(r.status)).length,
      stuck_runners: runners.filter((r) => r.status === 'stuck').length,
    };
  });
}

export function deriveWorkItems(db, viewerId = VIEWER.id) {
  return db
    .prepare('SELECT * FROM work_items WHERE viewer_id = ? ORDER BY id')
    .all(viewerId)
    .map((w) => ({
      id: w.id,
      repo_id: w.repo_id,
      kind: w.kind,
      title: w.title,
      state: w.state,
      priority: w.priority,
      url: w.url,
      metadata: JSON.parse(w.metadata_json),
    }));
}

const TABLES = [
  'viewers', 'viewer_permissions', 'feature_flags', 'repositories',
  'recent_repositories', 'pull_requests', 'pull_request_threads',
  'pull_request_checks', 'runner_pools', 'runners', 'runner_tasks', 'work_items',
];

export function deriveFixtures(db) {
  const row_counts = {};
  for (const t of TABLES) {
    row_counts[t] = db.prepare(`SELECT COUNT(*) AS n FROM ${t}`).get().n;
  }
  return {
    bootstrap: deriveBootstrap(db),
    repositories: deriveRepositories(db),
    pullRequests: derivePullRequests(db),
    fleet: deriveFleet(db),
    workItems: deriveWorkItems(db),
    manifest: {
      generated_at: GENERATED_AT,
      generator: 'db/seed/build-fixtures.mjs',
      source_migrations: db.__migrations ?? [],
      viewer: VIEWER.id,
      row_counts,
    },
  };
}

/** Build the store, derive fixtures, and write them to `outDir`. */
export function writeFixtures(outDir = DEFAULT_OUT_DIR) {
  const db = openSeededDb();
  try {
    const derived = deriveFixtures(db);
    const dir = typeof outDir === 'string' ? outDir : fileURLToPath(outDir);
    mkdirSync(dir, { recursive: true });
    const files = {
      'bootstrap.json': derived.bootstrap,
      'repositories.json': derived.repositories,
      'pull-requests.json': derived.pullRequests,
      'fleet.json': derived.fleet,
      'work-items.json': derived.workItems,
      'manifest.json': derived.manifest,
    };
    const written = [];
    for (const [name, value] of Object.entries(files)) {
      const target = new URL(name, dir.endsWith('/') ? pathToFileURL(dir) : pathToFileURL(`${dir}/`));
      writeFileSync(target, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
      written.push(fileURLToPath(target));
    }
    return { derived, written };
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// CLI entrypoint
// --------------------------------------------------------------------------

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  const { derived, written } = writeFixtures();
  const { row_counts } = derived.manifest;
  const total = Object.values(row_counts).reduce((a, b) => a + b, 0);
  process.stdout.write(
    `Applied ${derived.manifest.source_migrations.length} migration(s); ` +
      `seeded ${total} rows across ${Object.keys(row_counts).length} tables.\n`
  );
  for (const path of written) process.stdout.write(`  wrote ${path}\n`);
}
