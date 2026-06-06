// FleetPage.test.tsx — render + projection smoke for the /fleet operator page.
//
// Two tiers:
//   1. Pure projection (`fleetModel`): bootstrap snapshot folding, WS-event
//      overlay, freshness-window math, and the saturation/stuck/tag-starved
//      bottleneck derivation — all clock-independent.
//   2. Component render: drive `FleetPage` with a seeded bootstrap query +
//      a mocked `Event` payload in the realtime store and assert the page
//      paints pool cards, the stuck-runner banner, and the freshness badge.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';

import { FleetPage, FLEET_FRESHNESS_TTL_MS } from '../FleetPage';
import {
  applyFleetEvents,
  fleetStateFromBootstrap,
  isOutOfDate,
  poolFromRollup,
} from '../fleetModel';
import { BOOTSTRAP_QUERY_KEY } from '../../hooks/useBootstrap';
import { CONTROL_PLANE_RUNNERS_QUERY_KEY } from '../../hooks/useControlPlaneRunners';
import { useRealtimeStore } from '../../stores/realtimeStore';
import type {
  RunnerFabricResponse,
  WebBootstrap,
  WebEvent,
} from '../../api/types';

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

// ── Tier 2: component render ─────────────────────────────────────────────

const EMPTY_RUNNERS: RunnerFabricResponse = {
  schemaVersion: 'jeryu.runner_fabric/v1',
  local: {
    state: 'unknown',
    nodes: 0,
    onlineRunners: 0,
    offlineRunners: 0,
    busyRunners: 0,
    idleRunners: 0,
    totalSlots: 0,
    activeSlots: 0,
    utilization: 0,
    lastUpdated: null,
    nodeDetails: [],
  },
  mirror: {
    name: 'github_actions_runners',
    state: 'missing',
    reason: 'optional GitHub mirror runner adapter is not configured',
    docsUrl: 'docs/agent-native-standard.md',
  },
};

function renderFleet(tui: unknown, runners: RunnerFabricResponse = EMPTY_RUNNERS): void {
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
      workcells: false,
    },
  };
  client.setQueryData(BOOTSTRAP_QUERY_KEY, bootstrap);
  client.setQueryData(CONTROL_PLANE_RUNNERS_QUERY_KEY, runners);
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

  it('renders pool cards + system-health strip from bootstrap, with a freshness badge', () => {
    // No live event arrives in this test, and the bootstrap timestamp is far
    // in the past, so the freshness badge must appear.
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
    // Out of date because the only data is a 2020 bootstrap timestamp.
    expect(screen.getByTestId('fleet-freshness-badge')).toBeInTheDocument();
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
    // A fresh event timestamp means no freshness badge.
    expect(screen.queryByTestId('fleet-freshness-badge')).not.toBeInTheDocument();
  });

  it('renders the empty-pools roadmap note when no pools report', () => {
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet({
      generated_at: new Date().toISOString(),
      pool_activity: { repos: [], pools: [], unplaceable: [] },
      system: {},
    });
    expect(screen.getByTestId('fleet-page')).toBeInTheDocument();
    expect(
      screen.getByText(/No runner pools are reporting yet/i)
    ).toBeInTheDocument();
    expect(
      screen.getByText(/No system health reported yet/i)
    ).toBeInTheDocument();
    // No pools/components → the banner reports "Awaiting fleet telemetry."
    expect(screen.getByTestId('fleet-banner')).toHaveTextContent(
      /Awaiting fleet telemetry/i
    );
  });

  it('renders the runner-network drilldown from the control-plane runners payload', () => {
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet(
      {
        generated_at: new Date().toISOString(),
        pool_activity: { repos: [], pools: [], unplaceable: [] },
        system: {},
      },
      {
        schemaVersion: 'jeryu.runner_fabric/v1',
        local: {
          state: 'fresh',
          nodes: 2,
          onlineRunners: 2,
          offlineRunners: 0,
          busyRunners: 1,
          idleRunners: 1,
          totalSlots: 20,
          activeSlots: 20,
          utilization: 0.05,
          lastUpdated: '2026-06-05T00:05:00Z',
          nodeDetails: [
            {
              runnerId: 'xbabe0',
              source: 'runnerd',
              state: 'active',
              capacity: 10,
              inFlight: 1,
              labels: ['rust', 'dogfood'],
              classes: ['native-rust-clean'],
              activeTaskCount: 1,
              lastUpdated: '2026-06-05T00:05:00Z',
              activeTasks: [
                {
                  taskId: 'ar-1',
                  jobId: 'wc-1',
                  agentRunId: 'ar-1',
                  workcellId: 'wc-1',
                  repo: 'jeryu/veox',
                  label: 'editbot',
                  program: '/workspace/repair.sh',
                  state: 'running',
                  startedAt: '2026-06-05T00:00:00Z',
                  updatedAt: '2026-06-05T00:05:00Z',
                  ttyPreview: {
                    state: 'fresh',
                    lines: ['$ repair.sh', 'running tests', 'publishing patch'],
                  },
                },
              ],
            },
            {
              runnerId: 'local',
              source: 'local',
              state: 'active',
              capacity: 2,
              inFlight: 1,
              labels: ['local'],
              classes: ['native-rust-hot'],
              activeTaskCount: 1,
              lastUpdated: '2026-06-05T00:03:00Z',
              activeTasks: [
                {
                  taskId: 'ar-local',
                  jobId: 'wc-local',
                  agentRunId: 'ar-local',
                  workcellId: 'wc-local',
                  repo: null,
                  label: 'local-repair',
                  program: '/workspace/local.sh',
                  state: 'running',
                  startedAt: '2026-06-05T00:01:00Z',
                  updatedAt: '2026-06-05T00:03:00Z',
                  ttyPreview: {
                    state: 'missing',
                    lines: [],
                  },
                },
              ],
            },
          ],
        },
        mirror: {
          name: 'github_actions_runners',
          state: 'missing',
          reason: 'optional GitHub mirror runner adapter is not configured',
          docsUrl: 'docs/agent-native-standard.md',
        },
      }
    );

    expect(screen.getByTestId('fleet-network')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-node-board')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-node-box-xbabe0')).toHaveTextContent(
      'xbabe0'
    );
    expect(screen.getByTestId('fleet-node-xbabe0')).toHaveTextContent('xbabe0');
    expect(screen.getByTestId('fleet-node-local')).toHaveTextContent('local');
    expect(screen.getByTestId('fleet-task-ar-1')).toHaveTextContent(
      'publishing patch'
    );
    expect(screen.getByTestId('fleet-task-ar-local')).toHaveTextContent(
      /TTY preview unavailable/i
    );
  });

  it('paints a paused pool with its paused badge and no saturation class', () => {
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet({
      generated_at: new Date().toISOString(),
      pool_activity: {
        repos: [{ repo: 'a' }],
        pools: [rollup({ pool: 'trusted', paused: true, running_jobs: 0 })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    const card = screen.getByTestId('fleet-pool-trusted');
    expect(card).not.toHaveClass('is-saturated');
    expect(card).toHaveTextContent(/paused/i);
  });

  it('raises a status banner (no bottlenecks) when the fleet is healthy', () => {
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet({
      generated_at: new Date().toISOString(),
      pool_activity: {
        repos: [{ repo: 'a' }],
        pools: [rollup({ pool: 'trusted', running_jobs: 1, active_slots: 4 })],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    const banner = screen.getByTestId('fleet-banner');
    expect(banner).toHaveAttribute('role', 'status');
    expect(banner).toHaveTextContent(/All pools healthy/i);
  });

  it('flags a saturated pool with an alert banner and saturation class', () => {
    useRealtimeStore.setState({ events: [], status: 'open' });
    renderFleet({
      generated_at: new Date().toISOString(),
      pool_activity: {
        repos: [],
        pools: [
          rollup({
            pool: 'isolated',
            active_slots: 2,
            running_jobs: 2,
            queued_jobs: 5,
            online_runners: 2,
          }),
        ],
        unplaceable: [],
      },
      system: SYSTEM_HEALTH,
    });
    expect(screen.getByTestId('fleet-pool-isolated')).toHaveClass('is-saturated');
    const banner = screen.getByTestId('fleet-banner');
    expect(banner).toHaveAttribute('role', 'alert');
    expect(banner).toHaveTextContent(/saturated pool 'isolated'/);
  });
});
