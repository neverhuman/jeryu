// FileTree.tsx — lazy directory file browser (W-FE-10).
//
// Each row is one tree entry; directories may be expanded which lazy-loads
// their children via `useRepoTree(repoId, ref, path)`. Each expanded level
// mounts its own `Subtree`, so collapse drops the query and the tree stays
// cheap. Row heights are fixed at 28 px (kept in sync with browser.css's
// `.file-tree__row` height).
//
// Icons:
//   .md  → FileText
//   .ts/.tsx/.js/.jsx/.rs/.py/.go/.java/.c/.cpp/.h → FileCode
//   anything else → File
//   directory closed → Folder
//   directory open → FolderOpen
//
// The flat virtualized variant (`FlatFileList`) lives in `./FlatFileList`
// and is re-exported here so import sites keep working unchanged.

import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
} from 'lucide-react';
import { useMemo, useState } from 'react';

import { useRepoTree } from '../../hooks/useRepoTree';
import { INDENT_PX, fileIcon } from './fileTreeIcons';
import type { TreeEntry } from '../../api/types';

import './browser.css';

export { FlatFileList } from './FlatFileList';
export type { FlatFileListProps } from './FlatFileList';

export interface FileTreeProps {
  repoId: string | null;
  refName: string;
  /** Selected file path so the row is highlighted. */
  selectedPath?: string;
  /** Invoked when the user activates a file row. */
  onSelectFile: (entry: TreeEntry) => void;
}

function rootKey(path: string): string {
  return `r:${path}`;
}

/**
 * Inner sub-tree that fetches a single directory level lazily. Each instance
 * is mounted only when the parent is expanded, so collapse drops the query
 * and the tree stays cheap.
 */
function Subtree({
  repoId,
  refName,
  path,
  depth,
  expanded,
  toggle,
  selectedPath,
  onSelectFile,
}: {
  repoId: string | null;
  refName: string;
  path: string;
  depth: number;
  expanded: Set<string>;
  toggle: (path: string) => void;
  selectedPath: string | undefined;
  onSelectFile: (entry: TreeEntry) => void;
}): JSX.Element {
  const query = useRepoTree(repoId, refName, path);
  if (query.isPending) {
    return (
      <div className="file-tree__loading">Loading {path || '/'}…</div>
    );
  }
  if (query.isError) {
    return (
      <div className="file-tree__error">
        Could not load {path || '/'}.
      </div>
    );
  }
  const entries = query.data ?? [];
  return (
    <>
      {entries.map((entry) => (
        <FileTreeRow
          key={entry.path}
          entry={entry}
          depth={depth}
          expanded={expanded}
          toggle={toggle}
          selectedPath={selectedPath}
          onSelectFile={onSelectFile}
          repoId={repoId}
          refName={refName}
        />
      ))}
    </>
  );
}

function FileTreeRow({
  entry,
  depth,
  expanded,
  toggle,
  selectedPath,
  onSelectFile,
  repoId,
  refName,
}: {
  entry: TreeEntry;
  depth: number;
  expanded: Set<string>;
  toggle: (path: string) => void;
  selectedPath: string | undefined;
  onSelectFile: (entry: TreeEntry) => void;
  repoId: string | null;
  refName: string;
}): JSX.Element {
  const isDir = entry.kind === 'directory';
  const isOpen = isDir && expanded.has(entry.path);
  const isSelected = selectedPath === entry.path;
  const Icon = isDir ? (isOpen ? FolderOpen : Folder) : fileIcon(entry.name);
  return (
    <>
      <div
        className={`file-tree__row ${
          isSelected ? 'file-tree__row--selected' : ''
        }`}
        role="treeitem"
        aria-level={depth + 1}
        aria-expanded={isDir ? isOpen : undefined}
        aria-selected={isSelected || undefined}
        tabIndex={0}
        onClick={() => {
          if (isDir) toggle(entry.path);
          else onSelectFile(entry);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            if (isDir) toggle(entry.path);
            else onSelectFile(entry);
          } else if (e.key === 'ArrowRight' && isDir && !isOpen) {
            e.preventDefault();
            toggle(entry.path);
          } else if (e.key === 'ArrowLeft' && isDir && isOpen) {
            e.preventDefault();
            toggle(entry.path);
          }
        }}
        title={entry.path}
      >
        <span
          className="file-tree__indent"
          style={{ width: depth * INDENT_PX }}
          aria-hidden="true"
        />
        <span className="file-tree__chevron" aria-hidden="true">
          {isDir ? (
            isOpen ? (
              <ChevronDown size={12} />
            ) : (
              <ChevronRight size={12} />
            )
          ) : null}
        </span>
        <span className="file-tree__icon" aria-hidden="true">
          <Icon size={14} />
        </span>
        <span className="file-tree__name">{entry.name}</span>
      </div>
      {isDir && isOpen ? (
        <Subtree
          repoId={repoId}
          refName={refName}
          path={entry.path}
          depth={depth + 1}
          expanded={expanded}
          toggle={toggle}
          selectedPath={selectedPath}
          onSelectFile={onSelectFile}
        />
      ) : null}
    </>
  );
}

export function FileTree({
  repoId,
  refName,
  selectedPath,
  onSelectFile,
}: FileTreeProps): JSX.Element {
  // Expanded directory paths. The root path "" is always conceptually open.
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const toggle = useMemo(
    () => (path: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
    },
    []
  );

  return (
    <div
      className="file-tree"
      role="tree"
      aria-label="Repository files"
      key={rootKey(refName)}
    >
      <Subtree
        repoId={repoId}
        refName={refName}
        path=""
        depth={0}
        expanded={expanded}
        toggle={toggle}
        selectedPath={selectedPath}
        onSelectFile={onSelectFile}
      />
    </div>
  );
}
