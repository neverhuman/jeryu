// FleetPage.behavior.test.tsx — component render tier (live-event overlay and
// status/alert banners) for the /fleet operator page.
//
// Drive `FleetPage` with a seeded bootstrap query + a mocked `Event` payload in
// the realtime store and assert the stuck-runner banner, the paused-pool badge,
// the healthy status banner, and the saturated-pool alert banner.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';

import { FleetPage } from '../FleetPage';
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

// ── Tier 2: component render ─────────────────────────────────────────────

describe('FleetPage render', () => {
  afterEach(() => {
    // Reset the realtime singleton so events do not leak between tests.
    useRealtimeStore.setState({ events: [], status: 'idle' });
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
