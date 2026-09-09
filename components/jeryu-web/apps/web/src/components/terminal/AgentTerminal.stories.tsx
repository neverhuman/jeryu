import type { Meta, StoryObj } from '@storybook/react-vite';

import { AgentTerminal } from './AgentTerminal';
import { useRealtimeStore } from '../../stores/realtimeStore';

type RealtimeStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'reconnecting';

interface TerminalStoryArgs {
  runId: string;
  label?: string;
  status: RealtimeStatus;
}

function renderTerminalStory({ runId, label, status }: TerminalStoryArgs): JSX.Element {
  useRealtimeStore.setState({
    status,
    events: [],
    lastSeq: null,
    lastError: null,
    subscriptions: new Map(),
  });
  return (
    <div style={{ height: '420px', padding: '1rem', background: 'var(--color-bg-1, #0d1117)' }}>
      <AgentTerminal runId={runId} label={label} />
    </div>
  );
}

const meta = {
  title: 'Agents/AgentTerminal',
  parameters: { layout: 'fullscreen' },
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

/** A live run streaming its TTY: the status pill reads `open` and the toolbar
 *  exposes the byte budget + Ctrl-C interrupt. */
export const Live: Story = {
  render: () =>
    renderTerminalStory({
      runId: 'run-live',
      label: 'fix/parser · editbot',
      status: 'open',
    }),
};

/** A freshly launched run before the socket attaches — the "New Session" entry
 *  state, where the pill shows `connecting`. */
export const Connecting: Story = {
  render: () =>
    renderTerminalStory({
      runId: 'run-new',
      label: 'agent/session-new',
      status: 'connecting',
    }),
};

/** A detached / closed run: the pill shows `closed`, proving the danger
 *  variant renders accessibly. */
export const Closed: Story = {
  render: () =>
    renderTerminalStory({
      runId: 'run-idle',
      status: 'closed',
    }),
};
