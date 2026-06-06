import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { MemoryRouter } from 'react-router-dom';

import { BOOTSTRAP_QUERY_KEY } from '../hooks/useBootstrap';
import { CONTROL_PLANE_RUNNERS_QUERY_KEY } from '../hooks/useControlPlaneRunners';
import { FleetPage } from './FleetPage';
import { makeBootstrapFixture } from '../test/mocks';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { RunnerFabricResponse, WebBootstrap } from '../api/types';

type RealtimeStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'reconnecting';

interface FleetStoryArgs {
  bootstrap: WebBootstrap;
  status: RealtimeStatus;
}

const calmRunners: RunnerFabricResponse = {
  schemaVersion: 'jeryu.runner_fabric/v1',
  local: {
    state: 'fresh',
    nodes: 4,
    onlineRunners: 4,
    offlineRunners: 0,
    busyRunners: 1,
    idleRunners: 3,
    totalSlots: 40,
    activeSlots: 40,
    utilization: 0.025,
    lastUpdated: '2026-06-05T00:00:00Z',
    nodeDetails: [
      {
        runnerId: 'xbabe0',
        source: 'runnerd',
        state: 'active',
        capacity: 10,
        inFlight: 1,
        labels: ['rust', 'dogfood'],
        classes: ['native-rust-clean', 'native-rust-hot'],
        activeTaskCount: 1,
        lastUpdated: '2026-06-05T00:05:00Z',
        activeTasks: [
          {
            taskId: 'ar-000001',
            jobId: 'wc-0001',
            agentRunId: 'ar-000001',
            workcellId: 'wc-0001',
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
        runnerId: 'xbabe1',
        source: 'runnerd',
        state: 'draining',
        capacity: 10,
        inFlight: 0,
        labels: ['rust', 'dogfood'],
        classes: ['native-rust-clean', 'native-rust-hot'],
        activeTaskCount: 0,
        lastUpdated: '2026-06-05T00:05:00Z',
        activeTasks: [],
      },
      {
        runnerId: 'xbabe2',
        source: 'runnerd',
        state: 'dead',
        capacity: 10,
        inFlight: 0,
        labels: ['rust', 'dogfood'],
        classes: ['native-rust-clean', 'native-rust-hot'],
        activeTaskCount: 0,
        lastUpdated: null,
        activeTasks: [],
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
            taskId: 'ar-local-1',
            jobId: 'wc-local',
            agentRunId: 'ar-local-1',
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
};

function renderFleetStory({ bootstrap, status }: FleetStoryArgs): JSX.Element {
  useRealtimeStore.setState({
    status,
    events: [],
    lastSeq: null,
    lastError: null,
    subscriptions: new Map(),
  });

  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(BOOTSTRAP_QUERY_KEY, bootstrap);
  client.setQueryData(CONTROL_PLANE_RUNNERS_QUERY_KEY, calmRunners);

  return (
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <FleetPage />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

const calmBootstrap = makeBootstrapFixture({
  generated_at: '2026-06-02T00:00:00Z',
  tui: {
    generated_at: '2026-06-02T00:00:00Z',
    pool_activity: {
      repos: [{ repo: 'veox/redline' }],
      pools: [
        {
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
        },
      ],
      unplaceable: [],
      freshness: null,
    },
    system: {
      scm: { name: 'scm', status: 'healthy', latency_ms: 8, detail: null },
      database: {
        name: 'database',
        status: 'healthy',
        latency_ms: 2,
        detail: null,
      },
      sandbox: {
        name: 'sandbox',
        status: 'healthy',
        latency_ms: 5,
        detail: null,
      },
      cache: { name: 'cache', status: 'healthy', latency_ms: 1, detail: null },
      vault: { name: 'vault', status: 'healthy', latency_ms: 2, detail: null },
      runners: { online: 4, busy: 1, idle: 3, degraded: 0 },
    },
  },
});

const saturatedBootstrap = makeBootstrapFixture({
  generated_at: '2026-06-02T00:00:00Z',
  tui: {
    generated_at: '2026-06-02T00:00:00Z',
    pool_activity: {
      repos: [{ repo: 'veox/redline' }],
      pools: [
        {
          pool: 'isolated',
          tags: ['gpu'],
          trust_tier: 'isolated',
          paused: false,
          queued_jobs: 5,
          running_jobs: 2,
          failed_jobs: 0,
          active_slots: 2,
          configured_max_slots: 2,
          online_runners: 2,
          stuck_runners: 0,
        },
      ],
      unplaceable: [],
      freshness: null,
    },
    system: {
      scm: { name: 'scm', status: 'healthy', latency_ms: 8, detail: null },
      database: {
        name: 'database',
        status: 'healthy',
        latency_ms: 2,
        detail: null,
      },
      sandbox: {
        name: 'sandbox',
        status: 'degraded',
        latency_ms: null,
        detail: 'slow',
      },
      cache: { name: 'cache', status: 'healthy', latency_ms: 1, detail: null },
      vault: { name: 'vault', status: 'warning', latency_ms: null, detail: null },
      runners: { online: 2, busy: 2, idle: 0, degraded: 1 },
    },
  },
});

const meta = {
  title: 'Pages/FleetPage',
  parameters: { layout: 'fullscreen' },
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const CalmFleet: Story = {
  render: () => renderFleetStory({ bootstrap: calmBootstrap, status: 'open' }),
};

export const SaturatedFleet: Story = {
  render: () =>
    renderFleetStory({ bootstrap: saturatedBootstrap, status: 'open' }),
};
