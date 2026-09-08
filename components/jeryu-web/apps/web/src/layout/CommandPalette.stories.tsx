// CommandPalette.stories.tsx — palette states (W-T-07).
//
// `CommandPalette` is driven entirely by the Zustand `useCommandStore`.
// Each story seeds the store before render so the palette opens with the
// state under test: closed / open-empty / open-typing / open-many.
//
// `closed` is the production default — the palette renders nothing. The
// story still publishes the trigger affordance so reviewers see the
// shortcut hint.

import type { Meta, StoryObj } from '@storybook/react-vite';
import { useEffect } from 'react';

import { useCommandStore, type Command } from '../stores/commandStore';

import { CommandPalette } from './CommandPalette';

const NAVIGATION_COMMANDS: Command[] = [
  {
    id: 'nav.dashboard',
    title: 'Go to Dashboard',
    keywords: ['dashboard', 'home'],
    target: { kind: 'route', path: '/' },
    shortcut: 'g d',
  },
  {
    id: 'nav.repos',
    title: 'Go to Repositories',
    keywords: ['repos'],
    target: { kind: 'route', path: '/repos' },
    shortcut: 'g r',
  },
  {
    id: 'nav.search',
    title: 'Search…',
    keywords: ['search', 'find'],
    target: { kind: 'route', path: '/search' },
    shortcut: '/',
  },
];

const MANY_COMMANDS: Command[] = [
  ...NAVIGATION_COMMANDS,
  ...Array.from({ length: 12 }, (_, i) => ({
    id: `action.repo-${i}`,
    title: `Open repo veox/repo-${i}`,
    keywords: ['repo', `veox/repo-${i}`],
    target: { kind: 'route', path: `/repos/jeryu/veox/repo-${i}` } as const,
    riskTier: undefined,
  })),
  {
    id: 'pref.theme.dark',
    title: 'Theme: Dark',
    keywords: ['theme', 'dark'],
    target: { kind: 'action', actionId: 'pref.theme.dark' },
    run: () => undefined,
  },
];

function Seed({
  commands,
  open,
  query = '',
}: {
  commands: Command[];
  open: boolean;
  query?: string;
}): JSX.Element {
  const setState = useCommandStore.setState;
  useEffect(() => {
    setState({
      commands,
      isOpen: open,
      query,
    });
    return () => {
      // Reset the store between stories so they don't leak into each other.
      setState({ commands: [], isOpen: false, query: '' });
    };
  }, [commands, open, query, setState]);
  return <CommandPalette />;
}

const meta: Meta = {
  title: 'layout/CommandPalette',
  component: CommandPalette,
};
export default meta;

type Story = StoryObj;

export const Closed: Story = {
  render: () => (
    <div>
      <p style={{ color: 'var(--color-fg-muted)' }}>
        The palette is closed. Press <kbd>⌘K</kbd> to open it.
      </p>
      <Seed commands={NAVIGATION_COMMANDS} open={false} />
    </div>
  ),
};

export const OpenEmpty: Story = {
  name: 'Open, empty',
  render: () => <Seed commands={[]} open={true} />,
};

export const OpenTyping: Story = {
  name: 'Open, typing',
  render: () => (
    <Seed commands={NAVIGATION_COMMANDS} open={true} query="repo" />
  ),
};

export const OpenMany: Story = {
  name: 'Open, many results',
  render: () => <Seed commands={MANY_COMMANDS} open={true} />,
};
