// BranchSelector.tsx — branch + tag combobox via cmdk (W-FE-09).
//
// Renders a trigger button that toggles a `cmdk` palette filtered against
// `RefSelectorItem.name`. Selecting an item calls `onSelect(ref)`. The popover
// closes on selection, outside-click, and Escape.

import { Command } from 'cmdk';
import { ChevronDown, GitBranch, Tag } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { useRefs } from '../../hooks/useRefs';
import { ErrorState, LoadingState } from '../state';

import './browser.css';

export interface BranchSelectorProps {
  repoId: string | null;
  /** Currently selected ref name; rendered in the trigger button. */
  value: string;
  onSelect: (refName: string) => void;
  /** Defaults to "Switch branches/tags". */
  ariaLabel?: string;
}

export function BranchSelector({
  repoId,
  value,
  onSelect,
  ariaLabel = 'Switch branches/tags',
}: BranchSelectorProps): JSX.Element {
  const [open, setOpen] = useState(false);
  const [filterText, setFilterText] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);
  const refs = useRefs(repoId);

  useEffect(() => {
    if (!open) return undefined;
    const onClick = (e: MouseEvent): void => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="branch-selector" ref={containerRef}>
      <button
        type="button"
        className="branch-selector__trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${ariaLabel}: ${value}`}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="branch-selector__trigger-label">
          <GitBranch size={14} aria-hidden="true" />
          {value || 'choose a ref'}
        </span>
        <ChevronDown size={14} aria-hidden="true" />
      </button>
      {open ? (
        <Command
          className="branch-selector__panel"
          label={ariaLabel}
          loop
        >
          <Command.Input
            className="branch-selector__input"
            placeholder="Filter refs…"
            value={filterText}
            onValueChange={setFilterText}
            autoFocus
          />
          <Command.List className="branch-selector__list">
            {refs.isPending ? (
              <LoadingState
                title="Loading refs…"
                variant="message"
              />
            ) : refs.isError ? (
              <ErrorState
                title="Could not load refs"
                error={refs.error}
              />
            ) : (refs.data ?? []).length === 0 ? (
              <Command.Empty className="branch-selector__empty">
                No refs found.
              </Command.Empty>
            ) : (
              <>
                <Command.Empty className="branch-selector__empty">
                  No matches.
                </Command.Empty>
                {(refs.data ?? []).map((ref) => (
                  <Command.Item
                    key={`${ref.kind}:${ref.name}`}
                    value={`${ref.name} ${ref.kind}`}
                    className="branch-selector__item"
                    onSelect={() => {
                      onSelect(ref.name);
                      setOpen(false);
                    }}
                  >
                    <span className="branch-selector__item-label">
                      {ref.kind === 'tag' ? (
                        <Tag size={12} aria-hidden="true" />
                      ) : (
                        <GitBranch size={12} aria-hidden="true" />
                      )}
                      {ref.name}
                    </span>
                    <span className="branch-selector__item-kind">
                      {ref.kind}
                    </span>
                  </Command.Item>
                ))}
              </>
            )}
          </Command.List>
        </Command>
      ) : null}
    </div>
  );
}
