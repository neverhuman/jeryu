// AppShell.tsx — 2-column mission-control layout with collapsible sidebar.
//
//   ┌────────────────────────────────────────────────────┐
//   │ <GlobalHeader />                                    │
//   ├──────────┬─────────────────────────────────────────┤
//   │ <LeftNav  │ <Outlet />                              │
//   │   />      │                                         │
//   ├──────────┴─────────────────────────────────────────┤
//   │ <StatusBar />                                       │
//   └────────────────────────────────────────────────────┘
//
// Shell-level shortcuts (`⌘K` palette, `?` help) are wired here so they
// outlive any route change.

import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { Outlet } from 'react-router-dom';

import { CommandPalette } from './CommandPalette';
import { GlobalHeader } from './GlobalHeader';
import { LeftNav } from './LeftNav';
import { StatusBar } from './StatusBar';
import { useCommandStore } from '../stores/commandStore';
import { useKeyboardShortcut } from '../hooks/useKeyboard';
import { KeyboardShortcutsOverlay } from '../components/KeyboardShortcutsOverlay';
import { useShellCommands } from './useShellCommands';

import './AppShell.css';

export function AppShell(): JSX.Element {
  const navigate = useNavigate();
  const openPalette = useCommandStore((s) => s.open);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Register navigation commands so the palette is non-empty on first render.
  useShellCommands();

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((prev) => !prev);
  }, []);

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
      navigate('/search');
    },
    { label: 'Focus search', group: 'Navigation' }
  );

  useKeyboardShortcut('mod+b', toggleSidebar, {
    label: 'Toggle sidebar',
    group: 'Navigation',
  });

  useKeyboardShortcut('g d', () => navigate('/'), {
    label: 'Go to Dashboard',
    group: 'Navigation',
  });
  useKeyboardShortcut('g r', () => navigate('/repos'), {
    label: 'Go to Repositories',
    group: 'Navigation',
  });
  useKeyboardShortcut('g m', () => navigate('/pull-room'), {
    label: 'Go to Pull Room',
    group: 'Navigation',
  });
  useKeyboardShortcut('g s', () => navigate('/settings'), {
    label: 'Go to Settings',
    group: 'Navigation',
  });
  useKeyboardShortcut('Mod+/', () => navigate('/search'), {
    label: 'Go to Search',
    group: 'Navigation',
  });

  return (
    <div className={`app-shell${sidebarCollapsed ? ' app-shell--sidebar-collapsed' : ''}`}>
      <header className="app-shell__header">
        <GlobalHeader />
      </header>
      <aside className="app-shell__leftnav" aria-label="Primary navigation">
        <LeftNav />
        <button
          type="button"
          className="app-shell__sidebar-toggle"
          onClick={toggleSidebar}
          aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          title={`${sidebarCollapsed ? 'Expand' : 'Collapse'} sidebar (⌘B)`}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
            className="app-shell__sidebar-toggle-icon"
          >
            <path
              d="M10 4L6 8L10 12"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </aside>
      <main className="app-shell__main" id="main-content">
        <Outlet />
      </main>
      <footer className="app-shell__status">
        <StatusBar />
      </footer>
      <CommandPalette />
      <KeyboardShortcutsOverlay />
    </div>
  );
}
