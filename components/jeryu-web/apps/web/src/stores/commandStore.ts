// commandStore.ts — command-palette state (W-FE-05 + W-FE-14).
//
// The palette renders against this store; navigation commands are pushed by
// the keyboard layer. Command shape matches §35.2.3 — id/title/keywords/icon
// plus a `routeOrAction` discriminator so the registry can fan out to either
// navigation (most Phase 1 entries) or an Action (W-FE-13, future).

import { create } from 'zustand';

export interface CommandRouteTarget {
  kind: 'route';
  path: string;
}

export interface CommandActionTarget {
  kind: 'action';
  actionId: string;
}

export type CommandTarget = CommandRouteTarget | CommandActionTarget;

export type CommandRiskTier = 'low' | 'medium' | 'high' | 'critical';

export interface Command {
  id: string;
  title: string;
  description?: string;
  keywords: string[];
  /** Lucide icon name, resolved at render-time via lucide-react. */
  icon?: string;
  permission?: string;
  target: CommandTarget;
  shortcut?: string;
  riskTier?: CommandRiskTier;
  run?: () => void;
}

export interface CommandState {
  isOpen: boolean;
  query: string;
  commands: Command[];
  open: () => void;
  close: () => void;
  toggle: () => void;
  setQuery: (q: string) => void;
  register: (commands: Command[]) => void;
  unregister: (ids: string[]) => void;
  /** Convenience: return commands whose title/keywords match `query`. */
  results: () => Command[];
  /** Execute a command by id (run handler or navigate). */
  execute: (id: string, navigate: (path: string) => void) => void;
}

export const useCommandStore = create<CommandState>((set, get) => ({
  isOpen: false,
  query: '',
  commands: [],
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false, query: '' }),
  toggle: () => set((s) => ({ isOpen: !s.isOpen, query: '' })),
  setQuery: (query) => set({ query }),
  register: (commands) =>
    set((s) => {
      const incoming = new Map(s.commands.map((c) => [c.id, c]));
      for (const c of commands) incoming.set(c.id, c);
      return { commands: Array.from(incoming.values()) };
    }),
  unregister: (ids) =>
    set((s) => ({
      commands: s.commands.filter((c) => !ids.includes(c.id)),
    })),
  results: () => {
    const { query, commands } = get();
    if (!query.trim()) return commands;
    const needle = query.trim().toLowerCase();
    return commands.filter((c) => {
      if (c.title.toLowerCase().includes(needle)) return true;
      if (c.description?.toLowerCase().includes(needle)) return true;
      return c.keywords.some((kw) => kw.toLowerCase().includes(needle));
    });
  },
  execute: (id, navigate) => {
    const { commands, close } = get();
    const command = commands.find((c) => c.id === id);
    if (!command) return;
    close();
    if (command.run) {
      command.run();
      return;
    }
    if (command.target.kind === 'route') {
      navigate(command.target.path);
    }
    // 'action' targets are wired in W-FE-13 (ActionButton). Phase 1 ignores.
  },
}));
