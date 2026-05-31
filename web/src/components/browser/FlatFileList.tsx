// FlatFileList.tsx — virtualized flat-list variant of the file browser.
//
// Used when a flat list is preferable (file finder). Rows feed
// @tanstack/react-virtual so very large lists stay responsive; row heights
// are fixed at `ROW_HEIGHT_PX` (kept in sync with browser.css's
// `.file-tree__row` height). The lazy directory tree lives in `./FileTree`.

import { useVirtualizer } from '@tanstack/react-virtual';
import { useMemo, useRef } from 'react';

import { ROW_HEIGHT_PX, fileIcon } from './fileTreeIcons';
import type { TreeEntry } from '../../api/types';

import './browser.css';

interface FlatNode {
  entry: TreeEntry;
  depth: number;
  /** Stable rendering key — `${parentPath}/${name}` plus depth. */
  key: string;
}

export interface FlatFileListProps {
  entries: TreeEntry[];
  selectedPath?: string;
  onSelectFile: (entry: TreeEntry) => void;
  height?: number;
}

export function FlatFileList({
  entries,
  selectedPath,
  onSelectFile,
  height = 400,
}: FlatFileListProps): JSX.Element {
  const parentRef = useRef<HTMLDivElement>(null);
  const nodes: FlatNode[] = useMemo(
    () =>
      entries.map((entry, idx) => ({
        entry,
        depth: 0,
        key: `${entry.path}-${idx}`,
      })),
    [entries]
  );
  const virtualizer = useVirtualizer({
    count: nodes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 10,
  });
  return (
    <div
      className="file-tree"
      ref={parentRef}
      role="tree"
      style={{ height }}
      aria-label="File list"
    >
      <div
        style={{
          height: virtualizer.getTotalSize(),
          position: 'relative',
          width: '100%',
        }}
      >
        {virtualizer.getVirtualItems().map((vRow) => {
          const node = nodes[vRow.index];
          if (!node) return null;
          const isSelected = selectedPath === node.entry.path;
          const Icon = fileIcon(node.entry.name);
          return (
            <div
              key={node.key}
              className={`file-tree__row ${
                isSelected ? 'file-tree__row--selected' : ''
              }`}
              role="treeitem"
              aria-level={1}
              tabIndex={0}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${vRow.start}px)`,
                height: vRow.size,
              }}
              onClick={() => onSelectFile(node.entry)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onSelectFile(node.entry);
                }
              }}
            >
              <span className="file-tree__icon" aria-hidden="true">
                <Icon size={14} />
              </span>
              <span className="file-tree__name">{node.entry.path}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
