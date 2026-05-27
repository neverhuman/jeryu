// RepoFamilyGroup.tsx — labeled wrapper for grouped repos (W-FE-08).
//
// Groups are pure layout containers — the caller decides which `family`
// strings produce a group. We do not assume a hierarchy; flat groups keep
// the rendering simple and let `RepositoriesPage` decide whether to show
// "ungrouped" repositories under a generic label.

import type { ReactNode } from 'react';

import './repo.css';

export interface RepoFamilyGroupProps {
  /** Group title (e.g. `veox-*`); also used as the heading id stem. */
  title: string;
  /** Number of repositories inside the group; surfaced in the heading. */
  count: number;
  children: ReactNode;
}

function slugify(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export function RepoFamilyGroup({
  title,
  count,
  children,
}: RepoFamilyGroupProps): JSX.Element {
  const id = `repo-family-${slugify(title)}`;
  return (
    <section
      className="repo-family"
      aria-labelledby={id}
    >
      <header className="repo-family__head">
        <h2 className="repo-family__title" id={id}>
          {title}
        </h2>
        <span className="repo-family__count">{count} repos</span>
      </header>
      {children}
    </section>
  );
}
