// useShellCommands.ts — register the baseline navigation commands.

import { useEffect } from 'react';

import { useCommandStore, type Command } from '../stores/commandStore';
import { usePreferencesStore } from '../stores/preferencesStore';

export function useShellCommands(): void {
  const register = useCommandStore((s) => s.register);
  const unregister = useCommandStore((s) => s.unregister);
  const setTheme = usePreferencesStore((s) => s.setTheme);

  useEffect(() => {
    const commands: Command[] = [
      {
        id: 'nav.dashboard',
        title: 'Go to Dashboard',
        keywords: ['dashboard', 'home', 'attention'],
        icon: 'home',
        target: { kind: 'route', path: '/' },
        shortcut: 'g d',
      },
      {
        id: 'nav.repos',
        title: 'Go to Repositories',
        keywords: ['repos', 'repository', 'projects'],
        icon: 'folder',
        target: { kind: 'route', path: '/repos' },
        shortcut: 'g r',
      },
      {
        id: 'nav.work',
        title: 'Go to Work',
        keywords: ['work', 'tracker', 'issues', 'tasks', 'bugs'],
        icon: 'clipboard-list',
        target: { kind: 'route', path: '/work' },
        shortcut: 'g w',
      },
      {
        id: 'nav.pull-room',
        title: 'Go to Pull Room',
        keywords: ['pr', 'pull', 'merge', 'review'],
        icon: 'git-merge',
        target: { kind: 'route', path: '/pull-room' },
        shortcut: 'g m',
      },
      {
        id: 'nav.fleet',
        title: 'Go to Fleet',
        keywords: ['fleet', 'runners', 'utilization', 'saturation', 'health'],
        icon: 'server-cog',
        target: { kind: 'route', path: '/fleet' },
        shortcut: 'g f',
      },
      {
        id: 'nav.intelligence',
        title: 'Go to Intelligence',
        keywords: ['jmcp', 'control-plane', 'priority', 'graph'],
        icon: 'activity',
        target: { kind: 'route', path: '/intelligence' },
        shortcut: 'g i',
      },
      {
        id: 'nav.tools',
        title: 'Go to Tools',
        keywords: ['tools', 'duplicate', 'finder', 'loc', 'adoption'],
        icon: 'layers',
        target: { kind: 'route', path: '/tools' },
        shortcut: 'g t',
      },
      {
        id: 'nav.notifications',
        title: 'Go to Notifications',
        keywords: ['notify', 'alerts'],
        icon: 'bell',
        target: { kind: 'route', path: '/notifications' },
        shortcut: 'g n',
      },
      {
        id: 'nav.search',
        title: 'Search…',
        keywords: ['search', 'find', 'lookup', 'global'],
        icon: 'search',
        target: { kind: 'route', path: '/search' },
        shortcut: '/',
      },
      {
        id: 'nav.audit',
        title: 'Go to Audit',
        keywords: ['audit', 'compliance', 'logs'],
        icon: 'shield',
        target: { kind: 'route', path: '/audit' },
        shortcut: 'g a',
      },
      {
        id: 'nav.settings',
        title: 'Go to Admin Settings',
        keywords: ['settings', 'admin', 'preferences'],
        icon: 'cog',
        target: { kind: 'route', path: '/settings' },
        shortcut: 'g s',
      },
      {
        id: 'theme.light',
        title: 'Theme: Light',
        keywords: ['theme', 'appearance', 'light'],
        icon: 'sun',
        target: { kind: 'action', actionId: 'pref.theme.light' },
        run: () => setTheme('light'),
      },
      {
        id: 'theme.dark',
        title: 'Theme: Dark',
        keywords: ['theme', 'appearance', 'dark'],
        icon: 'moon',
        target: { kind: 'action', actionId: 'pref.theme.dark' },
        run: () => setTheme('dark'),
      },
      {
        id: 'theme.high-contrast',
        title: 'Theme: High contrast',
        keywords: ['theme', 'a11y', 'contrast'],
        icon: 'circle-dashed',
        target: { kind: 'action', actionId: 'pref.theme.high-contrast' },
        run: () => setTheme('high-contrast'),
      },
      {
        id: 'theme.system',
        title: 'Theme: System',
        keywords: ['theme', 'auto', 'system'],
        icon: 'monitor',
        target: { kind: 'action', actionId: 'pref.theme.system' },
        run: () => setTheme('system'),
      },
    ];
    register(commands);
    return () => unregister(commands.map((c) => c.id));
  }, [register, unregister, setTheme]);
}
