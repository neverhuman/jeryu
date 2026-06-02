import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { MemoryRouter } from 'react-router-dom';

import { BOOTSTRAP_QUERY_KEY } from '../hooks/useBootstrap';
import { FleetPage } from './FleetPage';
import { makeBootstrapFixture } from '../test/mocks';
import { useRealtimeStore } from '../stores/realtimeStore';
import type { WebBootstrap } from '../api/types';

type RealtimeStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'reconnecting';

interface FleetStoryArgs {
  bootstrap: WebBootstrap;
  status: RealtimeStatus;
}

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
