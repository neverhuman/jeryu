// RepoFamilyCard.tsx — clickable family tile for the repositories grid.
//
// Renders one tile per repository family with a worst-of health pill,
// member count, and summed activity meta (open PRs / failing checks /
// running jobs / last update). Clicking the tile drills down to the
// family page. Visually distinguished from plain repo cards by a
// stacked-deck offset shadow + accent border (see `.repo-family-card`).

import { Boxes, Gauge, GitMerge, Play, ShieldAlert } from 'lucide-react';
import { Link } from 'react-router-dom';

import type { FamilyRollup } from './familyRollup';

import { formatFamilyName } from './familyRollup';

import { RepoHealthPill } from './RepoHealthPill';
import { relativeTime } from './relativeTime';
import './repo.css';

export interface RepoFamilyCardProps {
  family: FamilyRollup;
}

export function familyHref(name: string): string {
  return `/repos/family/${encodeURIComponent(name)}`;
}

export function RepoFamilyCard({ family }: RepoFamilyCardProps): JSX.Element {
  return (
    <Link
      to={familyHref(family.name)}
      className="repo-card repo-family-card"
      aria-label={`Open family ${formatFamilyName(family.name)} (${family.memberCount} repositories)`}
    >
      <div className="repo-card__head">
        <h3 className="repo-card__title repo-family-card__title">
          <Boxes size={14} aria-hidden="true" />
          {formatFamilyName(family.name)}
        </h3>
        <RepoHealthPill health={family.health} />
      </div>
      <div>
        <span className="repo-family-card__count">
          {family.memberCount} repo{family.memberCount === 1 ? '' : 's'}
        </span>
      </div>
      <div className="repo-card__meta">
        <span
          className="repo-card__meta-item"
          title="Open pull requests"
          aria-label={`${family.openPullRequests} open pull requests`}
        >
          <GitMerge size={12} aria-hidden="true" /> {family.openPullRequests}
        </span>
        <span
          className="repo-card__meta-item"
          title="Failing checks"
          aria-label={`${family.failingChecks} failing checks`}
        >
          <ShieldAlert size={12} aria-hidden="true" /> {family.failingChecks}
        </span>
        <span
          className="repo-card__meta-item"
          title="Running jobs"
          aria-label={`${family.runningJobs} running jobs`}
        >
          <Play size={12} aria-hidden="true" /> {family.runningJobs}
        </span>
        {family.worstScore !== null ? (
          <span
            className="repo-card__meta-item"
            title="Lowest member jankurai score"
            aria-label={`Lowest member jankurai score ${family.worstScore}`}
          >
            <Gauge size={12} aria-hidden="true" /> {family.worstScore}
          </span>
        ) : null}
        <span className="repo-card__meta-item" title={family.updatedAt}>
          {relativeTime(family.updatedAt)}
        </span>
      </div>
    </Link>
  );
}
