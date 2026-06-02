// DiffFileTree.tsx — list of files changed in the PR (W-FE-11).
//
// One row per file, with:
//   * Status icon (added / modified / removed / renamed).
//   * Risk badge — when the backend tags a file with low / medium / high /
//     critical risk we surface it inline. Reviewers can spot critical files
//     before opening them.
//   * Viewed checkbox — local state only; remembered across navigation but
//     not yet persisted server-side (the §35.2.4 spec carries it client-side
//     for now).
//   * Additions / deletions counters.

import {
  CircleDot,
  FilePlus,
  FileX,
  GitMerge,
  Pencil,
  type LucideIcon,
} from 'lucide-react';
import type { ChangeEvent } from 'react';

import { RiskBadge, type RiskTier } from '../action/RiskBadge';
import type {
  PullRequestDiffFile,
  PullRequestFileStatus,
} from '../../api/types';

import './merge.css';

const STATUS_ICONS: Record<PullRequestFileStatus, LucideIcon> = {
  added: FilePlus,
  modified: Pencil,
  removed: FileX,
  renamed: GitMerge,
};

const STATUS_LABELS: Record<PullRequestFileStatus, string> = {
  added: 'Added',
  modified: 'Modified',
  removed: 'Removed',
  renamed: 'Renamed',
};

function riskTier(file: PullRequestDiffFile): RiskTier | null {
  if (!file.risk) return null;
  return file.risk;
}

export interface DiffFileTreeProps {
  files: PullRequestDiffFile[];
  /** Currently selected file path. */
  activePath: string | null;
  /** Set of file paths the user has marked as viewed. */
  viewedPaths: Set<string>;
  onSelect: (path: string) => void;
  onToggleViewed: (path: string, viewed: boolean) => void;
  className?: string;
}

export function DiffFileTree({
  files,
  activePath,
  viewedPaths,
  onSelect,
  onToggleViewed,
  className,
}: DiffFileTreeProps): JSX.Element {
  if (files.length === 0) {
    return (
      <div className={`diff-file-tree ${className ?? ''}`.trim()}>
        <p className="diff-file-tree__empty">No files changed.</p>
      </div>
    );
  }

  return (
    <nav
      className={`diff-file-tree ${className ?? ''}`.trim()}
      aria-label="Files changed"
    >
      <ul className="diff-file-tree__list">
        {files.map((file) => {
          const Icon = STATUS_ICONS[file.status] ?? CircleDot;
          const tier = riskTier(file);
          const isActive = file.path === activePath;
          const isViewed = viewedPaths.has(file.path);
          const checkboxId = `diff-viewed-${file.path.replace(/[^a-z0-9]/gi, '-')}`;
          const handleCheckbox = (event: ChangeEvent<HTMLInputElement>): void => {
            event.stopPropagation();
            onToggleViewed(file.path, event.target.checked);
          };
          return (
            <li key={file.path}>
              <div
                className={`diff-file-tree__row ${isActive ? 'diff-file-tree__row--active' : ''} ${isViewed ? 'diff-file-tree__row--viewed' : ''}`.trim()}
                data-status={file.status}
              >
                <button
                  type="button"
                  className="diff-file-tree__button"
                  onClick={() => onSelect(file.path)}
                  aria-current={isActive ? 'true' : undefined}
                >
                  <span
                    className={`diff-file-tree__status diff-file-tree__status--${file.status}`}
                    aria-label={STATUS_LABELS[file.status]}
                  >
                    <Icon aria-hidden="true" size={12} />
                  </span>
                  <span className="diff-file-tree__path">
                    {file.status === 'renamed' && file.old_path ? (
                      <>
                        <span className="diff-file-tree__prior-path">
                          {file.old_path}
                        </span>
                        <span aria-hidden="true"> → </span>
                      </>
                    ) : null}
                    {file.path}
                  </span>
                  <span className="diff-file-tree__counts">
                    <span className="diff-file-tree__additions">
                      +{file.additions}
                    </span>
                    <span className="diff-file-tree__deletions">
                      −{file.deletions}
                    </span>
                  </span>
                  {tier ? <RiskBadge tier={tier} /> : null}
                </button>
                <label
                  className="diff-file-tree__viewed"
                  htmlFor={checkboxId}
                  title="Mark as viewed"
                  onClick={(event) => event.stopPropagation()}
                >
                  <input
                    id={checkboxId}
                    type="checkbox"
                    checked={isViewed}
                    onChange={handleCheckbox}
                    aria-label={`Mark ${file.path} as viewed`}
                  />
                  <span className="diff-file-tree__viewed-label">Viewed</span>
                </label>
              </div>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
