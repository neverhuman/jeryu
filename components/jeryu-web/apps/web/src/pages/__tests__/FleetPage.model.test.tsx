// FleetPage.model.test.tsx — pure projection tier for the /fleet operator page.
//
// Pure projection (`fleetModel`): bootstrap snapshot folding, WS-event overlay,
// freshness-window math, and the saturation/stuck/tag-starved bottleneck
// derivation — all clock-independent.

import { describe, expect, it } from 'vitest';

import { FLEET_FRESHNESS_TTL_MS } from '../FleetPage';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isOutOfDate,
  poolFromRollup,
} from '../fleetModel';
import type { WebEvent } from '../../api/types';

// ── Fixtures ─────────────────────────────────────────────────────────────

function mkEvent(scope: string, payload: Record<string, unknown>): WebEvent {
  return {
    seq: BigInt(1),
    timestamp: new Date().toISOString(),
    scope,
    kind: `${scope}.snapshot`,
    entity: scope,
    summary: 'snapshot',
    payload,
  };
}

/** A `PoolRollup`-shaped JSON object (the wire shape over `pool.{name}`). */
function rollup(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    pool: 'trusted',
    tags: ['rust-hot'],
    trust_tier: 'trusted',
    paused: false,
    queued_jobs: 0,
    running_jobs: 1,
    failed_jobs: 0,
    active_slots: 4,
    configured_max_slots: 4,
    online_runners: 4,
    stuck_runners: 0,
    ...over,
  };
}

const SYSTEM_HEALTH = {
  scm: { name: 'scm', status: 'healthy', latency_ms: 12, detail: null },
  database: { name: 'database', status: 'healthy', latency_ms: 3, detail: null },
  sandbox: { name: 'sandbox', status: 'degraded', latency_ms: null, detail: 'slow' },
  cache: { name: 'cache', status: 'healthy', latency_ms: 1, detail: null },
  vault: { name: 'vault', status: 'warning', latency_ms: null, detail: null },
};

// ── Tier 1: pure projection ──────────────────────────────────────────────

describe('fleetModel projection', () => {
  it('derives idle slots, utilization, and saturation from a rollup', () => {
    const saturated = poolFromRollup(
      rollup({ active_slots: 2, running_jobs: 2, queued_jobs: 3 })
    );
    expect(saturated).not.toBeNull();
    expect(saturated?.idleSlots).toBe(0);
    expect(saturated?.utilization).toBe(1);
    expect(saturated?.saturated).toBe(true);

    const calm = poolFromRollup(rollup({ active_slots: 4, running_jobs: 1 }));
    expect(calm?.idleSlots).toBe(3);
    expect(calm?.utilization).toBeCloseTo(0.25);
    expect(calm?.saturated).toBe(false);
  });

  it('folds the bootstrap tui snapshot into pools + components + totals', () => {
    const state = fleetStateFromBootstrap({
      generated_at: '2026-05-31T00:00:00Z',
      pool_activity: {
        repos: [{ repo: 'veox/redline' }],
        pools: [rollup({ pool: 'trusted', running_jobs: 2, online_runners: 5 })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    expect(state.pools.map((p) => p.pool)).toEqual(['trusted']);
    expect(state.totals.runningJobs).toBe(2);
    expect(state.totals.repos).toBe(1);
    expect(state.components.map((c) => c.name)).toEqual([
      'scm',
      'database',
      'sandbox',
      'cache',
      'vault',
    ]);
    expect(state.health).toBe('healthy');
  });

  it('flags tag-starvation as a critical bottleneck', () => {
    const state = fleetStateFromBootstrap({
      pool_activity: {
        repos: [],
        pools: [rollup()],
        unplaceable: [{ tags: ['gpu'], count: 4 }],
      },
      system: SYSTEM_HEALTH,
    });
    expect(state.health).toBe('critical');
    expect(state.bottlenecks[0]).toMatch(/no pool serves it/);
  });

  it('overlays a later WS event on top of the bootstrap base', () => {
    const base = fleetStateFromBootstrap({
      pool_activity: { repos: [], pools: [rollup()], unplaceable: [] },
      system: SYSTEM_HEALTH,
    });
    const next = applyFleetEvents(base, [
      mkEvent('global.activity', {
        health: 'degraded',
        totals: {
          repos: 2,
          pools: 1,
          queued_jobs: 0,
          running_jobs: 1,
          failed_jobs: 0,
          online_runners: 3,
          stuck_runners: 2,
        },
        bottlenecks: ["2 runner(s) STUCK on pool 'trusted'"],
      }),
    ]);
    expect(next.health).toBe('degraded');
    expect(next.totals.stuckRunners).toBe(2);
    expect(next.bottlenecks).toContain("2 runner(s) STUCK on pool 'trusted'");
  });

  it('treats missing or outdated timestamps as out of date', () => {
    expect(isOutOfDate(null, FLEET_FRESHNESS_TTL_MS, 1000)).toBe(true);
    const fresh = new Date(1_000_000).toISOString();
    expect(isOutOfDate(fresh, FLEET_FRESHNESS_TTL_MS, 1_000_000 + 1_000)).toBe(
      false
    );
    expect(isOutOfDate(fresh, FLEET_FRESHNESS_TTL_MS, 1_000_000 + 60_000)).toBe(
      true
    );
  });

  it('returns the empty/unknown state when the bootstrap tui is absent', () => {
    for (const input of [undefined, null, {}, 'nope', 42]) {
      const state = fleetStateFromBootstrap(input);
      expect(state.health).toBe('unknown');
      expect(state.pools).toEqual([]);
      expect(state.components).toEqual([]);
      expect(state.bottlenecks).toEqual([]);
      expect(state.totals.pools).toBe(0);
      expect(state.lastUpdated).toBeNull();
    }
  });

  it('carries paused state through the rollup projection', () => {
    const paused = poolFromRollup(rollup({ paused: true, running_jobs: 0 }));
    expect(paused?.paused).toBe(true);
    // A paused pool with no queued work is not saturated.
    expect(paused?.saturated).toBe(false);
  });

  it('reports healthy with no bottlenecks for a calm fleet', () => {
    const state = fleetStateFromBootstrap({
      pool_activity: {
        repos: [{ repo: 'a' }, { repo: 'b' }],
        pools: [rollup({ pool: 'trusted', running_jobs: 1, active_slots: 4 })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    expect(state.health).toBe('healthy');
    expect(state.bottlenecks).toEqual([]);
    expect(state.totals.repos).toBe(2);
  });

  it('flags a saturated pool as a warning bottleneck', () => {
    const state = fleetStateFromBootstrap({
      pool_activity: {
        repos: [],
        pools: [
          rollup({
            pool: 'isolated',
            active_slots: 2,
            running_jobs: 2,
            queued_jobs: 5,
          }),
        ],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    expect(state.health).toBe('warning');
    expect(state.bottlenecks[0]).toMatch(/saturated pool 'isolated'/);
  });

  it('sorts pools by name and keeps later pool frames upserted (no activity frame)', () => {
    const base = fleetStateFromBootstrap({
      pool_activity: {
        repos: [],
        pools: [rollup({ pool: 'zeta' }), rollup({ pool: 'alpha' })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    const next = applyFleetEvents(base, [
      // Newest-first ordering (store prepends); the pool frame upserts 'beta'
      // and updates 'alpha' running_jobs, recomputing totals without an
      // activity frame.
      mkEvent('pool.beta', rollup({ pool: 'beta', running_jobs: 2 })),
      mkEvent('pool.alpha', rollup({ pool: 'alpha', running_jobs: 3, active_slots: 4 })),
    ]);
    expect(next.pools.map((p) => p.pool)).toEqual(['alpha', 'beta', 'zeta']);
    // Totals recomputed from the pool set (no global.activity frame arrived).
    expect(next.totals.runningJobs).toBe(2 + 3 + 1); // beta 2, alpha 3, zeta 1
  });

  it('utilization clamps to 1 and floors at 0 across edge rollups', () => {
    expect(poolFromRollup(rollup({ active_slots: 0, running_jobs: 0 }))?.utilization).toBe(0);
    // Over-subscribed (running > slots) clamps to 1, idle floors at 0.
    const over = poolFromRollup(rollup({ active_slots: 2, running_jobs: 5 }));
    expect(over?.utilization).toBe(1);
    expect(over?.idleSlots).toBe(0);
  });
});
