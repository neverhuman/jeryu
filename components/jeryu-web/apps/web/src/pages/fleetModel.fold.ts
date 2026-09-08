// fleetModel.fold.ts — fold the bootstrap `tui` snapshot and the live WS event
// buffer into the render-ready `FleetState`, plus the freshness gate the page
// badge uses. These are the top-level selectors `FleetPage.tsx` calls; the
// per-frame math lives in the parse/project sibling modules.

import type { WebEvent } from '../api/types';
import type {
  FleetHealth,
  FleetPool,
  FleetState,
  FleetTotals,
} from './fleetModel.types';
import { asHealth, asRecord, num, strList } from './fleetModel.parse';
import { componentsFromSystem, poolFromRollup } from './fleetModel.project';

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
    .filter((p): p is FleetPool => p !== undefined);
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
