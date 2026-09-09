// CommandPalette.tsx — global ⌘K palette (W-FE-14).
//
// Ships navigation + theme commands registered by `useShellCommands`.
// Action commands are wired by W-FE-13; repo lookup is added by W-FE-08.

import { Command } from 'cmdk';
import { useEffect, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { useCommandStore } from '../stores/commandStore';

export function CommandPalette(): JSX.Element {
  const isOpen = useCommandStore((s) => s.isOpen);
  const close = useCommandStore((s) => s.close);
  const query = useCommandStore((s) => s.query);
  const setQuery = useCommandStore((s) => s.setQuery);
  const commands = useCommandStore((s) => s.commands);
  const execute = useCommandStore((s) => s.execute);
  const navigate = useNavigate();

  useEffect(() => {
    if (!isOpen) return () => {};
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, close]);

  const groups = useMemo(() => {
    const buckets: Record<string, typeof commands> = {};
    for (const c of commands) {
      const group = c.target.kind === 'route' ? 'Navigation' : 'Preferences';
      buckets[group] ??= [];
      buckets[group].push(c);
    }
    return Object.entries(buckets);
  }, [commands]);

  if (!isOpen) return <></>;

  return (
    <div
      className="command-palette__backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <Command
        className="command-palette__panel"
        label="Command palette"
        loop
      >
        <Command.Input
          className="command-palette__input"
          aria-label="Command search"
          value={query}
          onValueChange={setQuery}
          autoFocus
        />
        <Command.List className="command-palette__list">
          <Command.Empty className="command-palette__empty">
            No matches found.
          </Command.Empty>
          {groups.map(([group, items]) => (
            <Command.Group
              key={group}
              heading={
                <span className="command-palette__group">{group}</span>
              }
            >
              {items.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.title} ${cmd.keywords.join(' ')}`}
                  className="command-palette__item"
                  onSelect={() => execute(cmd.id, (path) => navigate(path))}
                >
                  <span>{cmd.title}</span>
                  {cmd.shortcut ? (
                    <span
                      className="command-palette__hint"
                      aria-hidden="true"
                    >
                      {cmd.shortcut}
                    </span>
                  ) : null}
                </Command.Item>
              ))}
            </Command.Group>
          ))}
        </Command.List>
      </Command>
    </div>
  );
}
