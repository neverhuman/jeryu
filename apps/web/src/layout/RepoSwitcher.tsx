// RepoSwitcher.tsx — current-repo selector in the global header (W-FE-01).
//
// Displays the current repo id from `selectionStore` and opens the command
// palette so the user can navigate. The richer combobox fed by
// `useRepositories()` is layered on by W-FE-08.

import { FolderGit2 } from 'lucide-react';

import { useCommandStore } from '../stores/commandStore';
import { useSelectionStore } from '../stores/selectionStore';

export function RepoSwitcher(): JSX.Element {
  const currentRepoId = useSelectionStore((s) => s.currentRepoId);
  const openPalette = useCommandStore((s) => s.open);

  return (
    <button
      type="button"
      className="repo-switcher"
      onClick={() => openPalette()}
      aria-label="Switch repository"
    >
      <FolderGit2 size={14} aria-hidden="true" />
      <span className="repo-switcher__label">
        {currentRepoId ?? 'Choose repository'}
      </span>
      <span className="repo-switcher__hint" aria-hidden="true">
        ↕
      </span>
    </button>
  );
}
