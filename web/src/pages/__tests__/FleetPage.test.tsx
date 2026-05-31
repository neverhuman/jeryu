// FleetPage.test.tsx — render + projection smoke for the /fleet operator page.
//
// Two tiers:
//   1. Pure projection (`fleetModel`): bootstrap snapshot folding, WS-event
//      overlay, stale-TTL math, and the saturation/stuck/tag-starved
//      bottleneck derivation — all clock-independent.
//   2. Component render: drive `FleetPage` with a seeded bootstrap query +
//      a mocked `Event` payload in the realtime store and assert the page
//      paints pool cards, the stuck-runner banner, and the stale badge.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';

import { FleetPage, FLEET_STALE_TTL_MS } from '../FleetPage';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isStale,
  poolFromRollup,
} from '../fleetModel';
import { BOOTSTRAP_QUERY_KEY } from '../../hooks/useBootstrap';
import { useRealtimeStore } from '../../stores/realtimeStore';
import type { WebBootstrap, WebEvent } from '../../api/types';

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

  it('treats missing/old timestamps as stale', () => {
    expect(isStale(null, FLEET_STALE_TTL_MS, 1000)).toBe(true);
    const fresh = new Date(1_000_000).toISOString();
    expect(isStale(fresh, FLEET_STALE_TTL_MS, 1_000_000 + 1_000)).toBe(false);
    expect(isStale(fresh, FLEET_STALE_TTL_MS, 1_000_000 + 60_000)).toBe(true);
  });
});

// ── Tier 2: component render ─────────────────────────────────────────────

function renderFleet(tui: unknown): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const bootstrap: WebBootstrap = {
    generated_at: '2026-05-31T00:00:00Z',
    schema_version: '0.1.0-alpha',
    viewer: {
      id: 'local',
      login: 'local',
      display_name: 'Local',
      avatar_url: null,
      global_permissions: [],
    },
    tui: tui as Record<string, unknown>,
    recent_repositories: [],
    websocket_url: '/api/v1/ws',
    feature_flags: {
      repo_create: false,
      settings_write: false,
      merge_write: false,
      markdown_html: true,
      agents: false,
      mcp: false,
    },
  };
  client.setQueryData(BOOTSTRAP_QUERY_KEY, bootstrap);
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <FleetPage />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe('FleetPage render', () => {
  afterEach(() => {
    // Reset the realtime singleton so events do not leak between tests.
    useRealtimeStore.setState({ events: [], status: 'idle' });
  });

  it('renders pool cards + system-health strip from bootstrap, with a stale badge', () => {
    // No live event arrives in this test, and the bootstrap timestamp is far
    // in the past, so the stale badge must appear.
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet({
      generated_at: '2020-01-01T00:00:00Z',
      pool_activity: {
        repos: [{ repo: 'veox/redline' }],
        pools: [rollup({ pool: 'trusted' })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });

    expect(screen.getByTestId('fleet-page')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-pool-trusted')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-health-strip')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-health-sandbox')).toBeInTheDocument();
    // Stale because the only data is a 2020 bootstrap timestamp.
    expect(screen.getByTestId('fleet-stale-badge')).toBeInTheDocument();
  });

  it('surfaces a stuck-runner banner from a live Event payload', () => {
    useRealtimeStore.setState({
      status: 'open',
      events: [
        mkEvent('global.activity', {
          health: 'degraded',
          totals: {
            repos: 1,
            pools: 1,
            queued_jobs: 0,
            running_jobs: 1,
            failed_jobs: 0,
            online_runners: 4,
            stuck_runners: 3,
          },
          bottlenecks: ["3 runner(s) STUCK on pool 'trusted'"],
        }),
        mkEvent('pool.trusted', rollup({ stuck_runners: 3 })),
        mkEvent('system.health', SYSTEM_HEALTH),
      ],
    });
    renderFleet({
      generated_at: new Date().toISOString(),
      pool_activity: { repos: [], pools: [], unplaceable: [] },
      system: {},
    });

    const banner = screen.getByTestId('fleet-banner');
    expect(banner).toHaveAttribute('role', 'alert');
    expect(banner).toHaveTextContent(/STUCK on pool 'trusted'/);
    expect(banner).toHaveTextContent(/stuck runners/);
    // The pool card from the `pool.trusted` event renders with the stuck class.
    expect(screen.getByTestId('fleet-pool-trusted')).toHaveClass('is-stuck');
    // A fresh event timestamp means no stale badge.
    expect(screen.queryByTestId('fleet-stale-badge')).not.toBeInTheDocument();
  });
});
