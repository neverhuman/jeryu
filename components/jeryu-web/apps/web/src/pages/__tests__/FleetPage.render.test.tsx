// FleetPage.render.test.tsx — component render tier (pool cards, empty state,
// runner-network drilldown) for the /fleet operator page.
//
// Drive `FleetPage` with a seeded bootstrap query + a mocked control-plane
// runners payload and assert the page paints pool cards, the empty-pools
// roadmap note, the freshness badge, and the runner-network node board.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';

import { FleetPage } from '../FleetPage';
import { BOOTSTRAP_QUERY_KEY } from '../../hooks/useBootstrap';
import { CONTROL_PLANE_RUNNERS_QUERY_KEY } from '../../hooks/useControlPlaneRunners';
import { useRealtimeStore } from '../../stores/realtimeStore';
import type {
  RunnerFabricResponse,
  WebBootstrap,
} from '../../api/types';

// ── Fixtures ─────────────────────────────────────────────────────────────

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

function renderFleet(
  tui: unknown,
  runners: RunnerFabricResponse = EMPTY_RUNNERS,
  options: { runnersUpdatedAt?: number; runnersError?: Error } = {}
): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnMount: false } },
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
  client.setQueryData(CONTROL_PLANE_RUNNERS_QUERY_KEY, runners, {
    updatedAt: options.runnersUpdatedAt,
  });
  if (options.runnersError) {
    const query = client.getQueryCache().find({ queryKey: CONTROL_PLANE_RUNNERS_QUERY_KEY });
    if (!query) throw new Error('Runner cache entry missing');
    query.setState({ status: 'error', error: options.runnersError });
  }
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <FleetPage />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

// ── Tier 2: component render ─────────────────────────────────────────────

function observedRunners(): RunnerFabricResponse {
  return { ...EMPTY_RUNNERS, local: {
    ...EMPTY_RUNNERS.local, nodeDetails: [{
      runnerId: 'observed', source: 'workcell', state: 'active', capacity: 0,
      inFlight: 0, labels: [], classes: [], activeTaskCount: 1, lastUpdated: null,
      activeTasks: [{
        taskId: 'run-observed', jobId: 'cell-observed', agentRunId: 'run-observed',
        workcellId: 'cell-observed', repo: 'owner/repo', label: 'observed task',
        program: '/usr/bin/agent', state: 'running', startedAt: null,
        updatedAt: '2026-09-10T00:00:00Z',
        ttyPreview: { state: 'fresh', lines: ['observed output'] },
      }],
    }],
  } };
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
    expect(within(screen.getByTestId('fleet-pool-trusted')).queryByRole('progressbar'))
      .not.toBeInTheDocument();
    expect(screen.getByTestId('fleet-banner')).not.toHaveTextContent('All pools healthy');
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
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0%');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0 slots');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0 online');
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

  it('keeps observed jobs and tasks while capacity remains unknown after live frames', () => {
    const pool = rollup({ queued_jobs: 5, running_jobs: 2, failed_jobs: 3,
      active_slots: 0, configured_max_slots: 0, online_runners: 0 });
    useRealtimeStore.setState({ status: 'open', events: [{
      seq: BigInt(1), timestamp: new Date().toISOString(), scope: 'global.activity',
      kind: 'activity.snapshot', entity: 'global', summary: 'observed jobs',
      payload: { health: 'healthy', totals: { repos: 1, pools: 1,
        queued_jobs: 5, running_jobs: 2, failed_jobs: 3, online_runners: 0,
        stuck_runners: 0 }, bottlenecks: [] },
    }] });
    renderFleet({ generated_at: new Date().toISOString(), pool_activity: {
      freshness: { source: 'broker', state: 'unknown' },
      repos: [{ repo: 'owner/repo' }], pools: [pool], unplaceable: [],
    }, system: {} }, observedRunners());
    const card = screen.getByTestId('fleet-pool-trusted');
    expect(card).not.toHaveClass('is-saturated');
    expect(card).toHaveTextContent('capacity unknown');
    expect(within(card).queryByRole('progressbar')).not.toBeInTheDocument();
    expect(within(card).getByText('Queued').nextElementSibling).toHaveTextContent('5');
    expect(within(card).getByText('Running').nextElementSibling).toHaveTextContent('2');
    expect(within(card).getByText('Failed').nextElementSibling).toHaveTextContent('3');
    expect(screen.getByTestId('fleet-banner')).not.toHaveTextContent('All pools healthy');
    expect(screen.getByTestId('fleet-banner')).not.toHaveAttribute('role', 'alert');
    const node = screen.getByTestId('fleet-node-observed');
    expect(node).toHaveTextContent(/Capacity\s*unknown/);
    expect(screen.getByTestId('fleet-node-box-observed')).toHaveClass('is-unknown', 'is-active');
    expect(screen.getByTestId('fleet-node-box-observed').querySelector('.fleet__slot-grid')).toBeNull();
    expect(screen.getByTestId('fleet-task-run-observed')).toHaveTextContent('observed output');
    expect(screen.getByTestId('fleet-task-run-observed')).toHaveAttribute('href',
      '/repos/jeryu/owner%2Frepo/agents/run-observed');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0%');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0 slots');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('0 online');
  });

  it('hides capacity from expired runner cache while retaining task observations', () => {
    const runners = observedRunners();
    runners.local.state = 'fresh';
    runners.local.nodeDetails[0].source = 'runnerd';
    runners.local.nodeDetails[0].capacity = 8;
    renderFleet({}, runners, { runnersUpdatedAt: Date.now() - 60_000 });
    expect(screen.getByTestId('fleet-page')).toHaveTextContent('Runner capacity unknown');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('8 slots');
    expect(screen.getByTestId('fleet-node-observed')).toHaveTextContent(/Capacity\s*unknown/);
    expect(screen.getByTestId('fleet-task-run-observed')).toHaveTextContent('observed output');
  });

  it('does not leave cached capacity visible after a runner query failure', () => {
    const runners = observedRunners();
    runners.local.state = 'fresh';
    runners.local.nodeDetails[0].source = 'runnerd';
    runners.local.nodeDetails[0].capacity = 8;
    renderFleet({}, runners, { runnersError: new Error('Runner request failed') });
    expect(screen.getByText('Runner request failed')).toBeInTheDocument();
    expect(screen.getByTestId('fleet-page')).toHaveTextContent('Runner capacity unknown');
    expect(screen.getByTestId('fleet-page')).not.toHaveTextContent('8 slots');
  });

  it('keeps a measured zero distinguishable from unknown pool capacity', () => {
    renderFleet({ generated_at: new Date().toISOString(), pool_activity: {
      freshness: { source: 'broker', state: 'fresh', observed_at: new Date().toISOString() },
      repos: [{ repo: 'owner/repo' }], pools: [rollup({ running_jobs: 0,
        active_slots: 0, configured_max_slots: 0, online_runners: 0 })], unplaceable: [],
    }, system: {} }, { ...EMPTY_RUNNERS, local: {
      ...EMPTY_RUNNERS.local, state: 'fresh', nodes: 1, onlineRunners: 1,
      idleRunners: 1, nodeDetails: [{
        runnerId: 'zero-capacity', source: 'runnerd', state: 'active', capacity: 0,
        inFlight: 0, labels: [], classes: [], activeTaskCount: 0,
        lastUpdated: null, activeTasks: [],
      }],
    } });
    const card = screen.getByTestId('fleet-pool-trusted');
    expect(within(card).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
    expect(card).toHaveTextContent('0%');
    expect(card).not.toHaveTextContent('capacity unknown');
    expect(screen.getByTestId('fleet-banner')).toHaveTextContent('All pools healthy');
    expect(screen.getByTestId('fleet-page')).toHaveTextContent('0 slots');
    const zeroNode = screen.getByTestId('fleet-node-box-zero-capacity');
    expect(zeroNode).toHaveTextContent('0/0');
    expect(zeroNode.querySelectorAll('.fleet__slot')).toHaveLength(0);
  });

  it.each(['system.health', 'global.activity'])(
    'does not refresh expired capacity from a fresh %s frame',
    (scope) => {
      useRealtimeStore.setState({ status: 'open', events: [{
        seq: BigInt(1), timestamp: new Date().toISOString(), scope,
        kind: 'snapshot', entity: 'global', summary: 'new activity',
        payload: scope === 'system.health' ? SYSTEM_HEALTH : {
          health: 'healthy', totals: { repos: 1, pools: 1, queued_jobs: 0,
            running_jobs: 1, failed_jobs: 0, online_runners: 4, stuck_runners: 0 },
          bottlenecks: [],
        },
      }] });
      renderFleet({ generated_at: '2020-01-01T00:00:00Z', pool_activity: {
        repos: [{ repo: 'owner/repo' }], pools: [rollup()], unplaceable: [],
      }, system: {} });
      const card = screen.getByTestId('fleet-pool-trusted');
      expect(card).toHaveTextContent('capacity unknown');
      expect(within(card).queryByRole('progressbar')).not.toBeInTheDocument();
      expect(screen.getByTestId('fleet-banner')).not.toHaveTextContent('All pools healthy');
      expect(screen.getByTestId('fleet-freshness-badge')).toBeInTheDocument();
    }
  );
});
