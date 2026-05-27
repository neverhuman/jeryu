// AppShell.tsx — 3-column mission-control layout (W-FE-01).
//
//   ┌────────────────────────────────────────────────────────────────────┐
//   │ <GlobalHeader />                                                    │
//   ├──────────┬────────────────────────────────────────┬─────────────────┤
//   │ <LeftNav │ <Outlet />                              │ <LiveActivity  │
//   │   />     │                                         │   Dock />      │
//   ├──────────┴────────────────────────────────────────┴─────────────────┤
//   │ <StatusBar />                                                       │
//   └────────────────────────────────────────────────────────────────────┘
//
// Shell-level shortcuts (`⌘K` palette, `?` help) are wired here so they
// outlive any route change.

import { useNavigate } from 'react-router-dom';

import { Outlet } from 'react-router-dom';

import { CommandPalette } from './CommandPalette';
import { GlobalHeader } from './GlobalHeader';
import { LeftNav } from './LeftNav';
import { LiveActivityDock } from './LiveActivityDock';
import { StatusBar } from './StatusBar';
import { useCommandStore } from '../stores/commandStore';
import { useKeyboardShortcut } from '../hooks/useKeyboard';
import { KeyboardShortcutsOverlay } from '../components/KeyboardShortcutsOverlay';
import { useShellCommands } from './useShellCommands';

import './AppShell.css';

export function AppShell(): JSX.Element {
  const navigate = useNavigate();
  const openPalette = useCommandStore((s) => s.open);
  // Register navigation commands so the palette is non-empty on first render.
  useShellCommands();

  useKeyboardShortcut(
    'mod+k',
    () => {
      openPalette();
    },
    { label: 'Open command palette', group: 'Navigation' }
  );

  useKeyboardShortcut(
    '/',
    () => {
      // Phase 1: navigate to the search stub. W-FE-08 wires the real
      // search input.
      navigate('/?focus=search');
    },
    { label: 'Focus search', group: 'Navigation' }
  );

  useKeyboardShortcut('g d', () => navigate('/'), {
    label: 'Go to Dashboard',
    group: 'Navigation',
  });
  useKeyboardShortcut('g r', () => navigate('/repos'), {
    label: 'Go to Repositories',
    group: 'Navigation',
  });
  useKeyboardShortcut('g m', () => navigate('/merge-room'), {
    label: 'Go to Merge Room',
    group: 'Navigation',
  });
  useKeyboardShortcut('g s', () => navigate('/settings'), {
    label: 'Go to Settings',
    group: 'Navigation',
  });

  return (
    <div className="app-shell">
      <header className="app-shell__header">
        <GlobalHeader />
      </header>
      <aside className="app-shell__leftnav" aria-label="Primary navigation">
        <LeftNav />
      </aside>
      <main className="app-shell__main" id="main-content">
        <Outlet />
      </main>
      <aside className="app-shell__activity" aria-label="Live activity">
        <LiveActivityDock />
      </aside>
      <footer className="app-shell__status">
        <StatusBar />
      </footer>
      <CommandPalette />
      <KeyboardShortcutsOverlay />
    </div>
  );
}
