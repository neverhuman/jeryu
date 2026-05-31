// fleetModel.ts — pure selectors that fold the live WS event buffer + the
// bootstrap `tui` snapshot into a normalized, render-ready fleet view.
//
// The /fleet page surfaces the server-wide runner-pool fabric across ALL
// repos: per-pool utilization, queued/running/failed jobs, online/stuck
// runners, saturation/pause state, the system-health strip, and the derived
// bottleneck banner. Two data sources feed it and both speak the same Rust
// read-model contract (`jeryu_readmodel::pool_activity` + `SystemHealth`):
//
//   * The bootstrap (`WebBootstrap.tui.pool_activity` + `tui.system`) gives
//     the structured first-paint snapshot so the page is never empty.
//   * The WebSocket event spine streams `WebEvent` frames for the scopes
//     `global.activity` (totals + health + bottleneck strings),
//     `pool.{name}` (one `PoolRollup`), and `system.health` (`SystemHealth`).
//
// Keeping the projection here (a pure function of inputs, no React, no store)
// makes the fleet math unit-testable in isolation and keeps `FleetPage.tsx`
// a thin renderer. The slot/utilization/saturation rules mirror the Rust
// `PoolRollup` selectors so the web surface cannot disagree with the TUI.

import type { WebEvent } from '../api/types';

/** Health level mirrors `jeryu_readmodel::HealthLevel` (snake_case wire). */
export type FleetHealth =
  | 'healthy'
  | 'warning'
  | 'degraded'
  | 'critical'
  | 'unknown';

const HEALTH_VALUES: ReadonlySet<string> = new Set([
  'healthy',
  'warning',
  'degraded',
  'critical',
  'unknown',
]);

/** A single runner pool, rolled up across every repo on the server. */
export interface FleetPool {
  pool: string;
  tags: string[];
  trustTier: string;
  paused: boolean;
  queuedJobs: number;
  runningJobs: number;
  failedJobs: number;
  activeSlots: number;
  configuredMaxSlots: number;
  onlineRunners: number;
  stuckRunners: number;
  /** Derived: slots free for new work (active − running, floored at 0). */
  idleSlots: number;
  /** Derived: running / active, clamped 0..1 (0 when no active slots). */
  utilization: number;
  /** Derived: queued work with no idle slot to take it. */
  saturated: boolean;
}

/** Server-wide totals (mirrors `ActivityTotals`). */
export interface FleetTotals {
  repos: number;
  pools: number;
  queuedJobs: number;
  runningJobs: number;
  failedJobs: number;
  onlineRunners: number;
  stuckRunners: number;
}

/** One system-health component (mirrors `ComponentHealth`). */
export interface FleetComponent {
  name: string;
  status: FleetHealth;
  latencyMs: number | null;
  detail: string | null;
}

/** The fully-folded view the page renders. */
export interface FleetState {
  /** Overall pool-fabric health. */
  health: FleetHealth;
  totals: FleetTotals;
  pools: FleetPool[];
  /** Human-readable bottleneck lines, most severe first. */
  bottlenecks: string[];
  /** scm/database/sandbox/cache/vault component health. */
  components: FleetComponent[];
  /** Online-but-stuck runner total across the server. */
  stuckRunnerTotal: number;
  /** Fleet-wide utilization (running / active across all pools), 0..1. */
  utilization: number;
  /** Most recent timestamp seen across the inputs, or null if none. */
  lastUpdated: string | null;
}

/** Component order for the system-health strip. */
const COMPONENT_ORDER = ['scm', 'database', 'sandbox', 'cache', 'vault'];

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : null;
}

function num(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function strList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
}

export function asHealth(value: unknown): FleetHealth {
  return typeof value === 'string' && HEALTH_VALUES.has(value)
    ? (value as FleetHealth)
    : 'unknown';
}

/** Project a raw `PoolRollup` JSON object into a `FleetPool` with derived math. */
export function poolFromRollup(raw: unknown): FleetPool | null {
  const r = asRecord(raw);
  if (!r) return null;
  const pool = str(r.pool);
  if (!pool) return null;
  const queuedJobs = num(r.queued_jobs);
  const runningJobs = num(r.running_jobs);
  const activeSlots = num(r.active_slots);
  const idleSlots = Math.max(0, activeSlots - runningJobs);
  const utilization =
    activeSlots === 0 ? 0 : Math.min(1, Math.max(0, runningJobs / activeSlots));
  return {
    pool,
    tags: strList(r.tags),
    trustTier: str(r.trust_tier) || 'trusted',
    paused: r.paused === true,
    queuedJobs,
    runningJobs,
    failedJobs: num(r.failed_jobs),
    activeSlots,
    configuredMaxSlots: num(r.configured_max_slots),
    onlineRunners: num(r.online_runners),
    stuckRunners: num(r.stuck_runners),
    idleSlots,
    utilization,
    saturated: queuedJobs > 0 && idleSlots === 0,
  };
}

/** Project a raw `ComponentHealth` JSON object into a `FleetComponent`. */
function componentFromRaw(name: string, raw: unknown): FleetComponent {
  const r = asRecord(raw);
  const latency = r ? r.latency_ms : null;
  const detail = r ? r.detail : null;
  return {
    name: r && typeof r.name === 'string' ? r.name : name,
    status: asHealth(r?.status),
    latencyMs:
      typeof latency === 'number' && Number.isFinite(latency) ? latency : null,
    detail: typeof detail === 'string' ? detail : null,
  };
}

/** Read the ordered scm/database/sandbox/cache/vault strip from a `SystemHealth`. */
export function componentsFromSystem(raw: unknown): FleetComponent[] {
  const sys = asRecord(raw);
  if (!sys) return [];
  return COMPONENT_ORDER.filter((name) => name in sys).map((name) =>
    componentFromRaw(name, sys[name])
  );
}

function totalsFromActivity(pools: FleetPool[], repos: number): FleetTotals {
  return pools.reduce<FleetTotals>(
    (acc, p) => ({
      repos,
      pools: pools.length,
      queuedJobs: acc.queuedJobs + p.queuedJobs,
      runningJobs: acc.runningJobs + p.runningJobs,
      failedJobs: acc.failedJobs + p.failedJobs,
      onlineRunners: acc.onlineRunners + p.onlineRunners,
      stuckRunners: acc.stuckRunners + p.stuckRunners,
    }),
    {
      repos,
      pools: pools.length,
      queuedJobs: 0,
      runningJobs: 0,
      failedJobs: 0,
      onlineRunners: 0,
      stuckRunners: 0,
    }
  );
}

/** Mirror `PoolActivity::bottlenecks().describe()` ordering for first paint. */
function bottlenecksFromActivity(
  pools: FleetPool[],
  unplaceable: { tags: string[]; count: number }[]
): string[] {
  const critical: string[] = [];
  const degraded: string[] = [];
  const warning: string[] = [];
  for (const demand of unplaceable) {
    if (demand.count > 0) {
      critical.push(
        `${demand.count} job(s) waiting for tag [${demand.tags.join(', ')}] — no pool serves it`
      );
    }
  }
  for (const p of pools) {
    if (p.saturated) {
      warning.push(
        `${p.queuedJobs} job(s) queued on saturated pool '${p.pool}' (${Math.round(
          p.utilization * 100
        )}% slots busy)`
      );
    }
    if (p.stuckRunners > 0) {
      degraded.push(`${p.stuckRunners} runner(s) STUCK on pool '${p.pool}'`);
    }
  }
  return [...critical, ...degraded, ...warning];
}

function healthFromActivity(
  pools: FleetPool[],
  repos: number,
  unplaceable: { count: number }[]
): FleetHealth {
  if (pools.length === 0 && repos === 0) return 'unknown';
  if (unplaceable.some((d) => d.count > 0)) return 'critical';
  if (pools.some((p) => p.saturated)) return 'warning';
  if (pools.some((p) => p.stuckRunners > 0)) return 'degraded';
  return 'healthy';
}

function fleetUtilization(pools: FleetPool[]): number {
  let active = 0;
  let running = 0;
  for (const p of pools) {
    active += p.activeSlots;
    running += p.runningJobs;
  }
  return active === 0 ? 0 : Math.min(1, running / active);
}

function emptyState(): FleetState {
  return {
    health: 'unknown',
    totals: {
      repos: 0,
      pools: 0,
      queuedJobs: 0,
      runningJobs: 0,
      failedJobs: 0,
      onlineRunners: 0,
      stuckRunners: 0,
    },
    pools: [],
    bottlenecks: [],
    components: [],
    stuckRunnerTotal: 0,
    utilization: 0,
    lastUpdated: null,
  };
}

/**
 * Build the first-paint state from the bootstrap `tui` JSON. The shape is the
 * serialized `TuiReadModel`; we read `tui.pool_activity` (a `PoolActivity`)
 * and `tui.system` (a `SystemHealth`). Anything missing degrades gracefully.
 */
export function fleetStateFromBootstrap(tui: unknown): FleetState {
  const root = asRecord(tui);
  if (!root) return emptyState();
  const activity = asRecord(root.pool_activity);
  const poolsRaw = activity && Array.isArray(activity.pools) ? activity.pools : [];
  const reposRaw = activity && Array.isArray(activity.repos) ? activity.repos : [];
  const unplaceableRaw =
    activity && Array.isArray(activity.unplaceable) ? activity.unplaceable : [];

  const pools = poolsRaw
    .map(poolFromRollup)
    .filter((p): p is FleetPool => p !== null);
  const unplaceable = unplaceableRaw.map((d) => {
    const r = asRecord(d);
    return { tags: strList(r?.tags), count: num(r?.count) };
  });
  const components = componentsFromSystem(root.system);
  const totals = totalsFromActivity(pools, reposRaw.length);

  return {
    health: healthFromActivity(pools, reposRaw.length, unplaceable),
    totals,
    pools,
    bottlenecks: bottlenecksFromActivity(pools, unplaceable),
    components,
    stuckRunnerTotal: totals.stuckRunners,
    utilization: fleetUtilization(pools),
    lastUpdated: typeof root.generated_at === 'string' ? root.generated_at : null,
  };
}

/**
 * Fold the rolling WS event buffer onto a base state. Events arrive
 * newest-first (the realtime store prepends), so we walk oldest→newest and
 * let later frames win. `global.activity` replaces totals/health/bottleneck
 * lines; `pool.{name}` upserts a single pool; `system.health` replaces the
 * component strip. `lastUpdated` tracks the newest applied frame.
 */
export function applyFleetEvents(
  base: FleetState,
  events: readonly WebEvent[]
): FleetState {
  if (events.length === 0) return base;

  // Mutable working copy keyed by pool name so repeated pool frames upsert.
  let health = base.health;
  let totals = base.totals;
  let bottlenecks = base.bottlenecks;
  let components = base.components;
  let lastUpdated = base.lastUpdated;
  const poolMap = new Map<string, FleetPool>(base.pools.map((p) => [p.pool, p]));

  // Oldest → newest so the most recent frame wins.
  const ordered = [...events].reverse();
  for (const evt of ordered) {
    const payload = asRecord(evt.payload);
    if (!payload) continue;
    if (evt.scope === 'global.activity') {
      health = asHealth(payload.health);
      const t = asRecord(payload.totals);
      if (t) {
        totals = {
          repos: num(t.repos),
          pools: num(t.pools),
          queuedJobs: num(t.queued_jobs),
          runningJobs: num(t.running_jobs),
          failedJobs: num(t.failed_jobs),
          onlineRunners: num(t.online_runners),
          stuckRunners: num(t.stuck_runners),
        };
      }
      bottlenecks = strList(payload.bottlenecks);
      lastUpdated = evt.timestamp;
    } else if (evt.scope.startsWith('pool.')) {
      const pool = poolFromRollup(payload);
      if (pool) {
        poolMap.set(pool.pool, pool);
        lastUpdated = evt.timestamp;
      }
    } else if (evt.scope === 'system.health') {
      const next = componentsFromSystem(payload);
      if (next.length > 0) {
        components = next;
        lastUpdated = evt.timestamp;
      }
    }
  }

  const pools = [...poolMap.values()].sort((a, b) => a.pool.localeCompare(b.pool));
  // The activity frame carries authoritative totals; if none arrived, recompute
  // from the (possibly pool-frame-updated) pool set so the header stays honest.
  const sawActivity = events.some((e) => e.scope === 'global.activity');
  const resolvedTotals = sawActivity
    ? totals
    : totalsFromActivity(pools, totals.repos);

  return {
    health,
    totals: resolvedTotals,
    pools,
    bottlenecks,
    components,
    stuckRunnerTotal: resolvedTotals.stuckRunners,
    utilization: fleetUtilization(pools),
    lastUpdated,
  };
}

/**
 * Freshness-gap check: true when `lastUpdated` is older than `ttlMs` relative
 * to `now`. A null/unparseable timestamp is treated as out of date (we have no
 * proof of freshness). Used to drive the freshness badge on the page.
 */
export function isOutOfDate(
  lastUpdated: string | null,
  ttlMs: number,
  now: number = Date.now()
): boolean {
  if (!lastUpdated) return true;
  const t = Date.parse(lastUpdated);
  if (Number.isNaN(t)) return true;
  return now - t > ttlMs;
}
