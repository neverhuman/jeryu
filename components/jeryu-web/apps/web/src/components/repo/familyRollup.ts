// familyRollup.ts — pure aggregation helpers for repository families.
//
// A "family" is a backend-assigned grouping label (`RepositorySummary.family`,
// e.g. `jmcp-split`). These helpers fold a list of summaries into per-family
// rollups for the family tiles and the family drill-down page. No React, no
// I/O — fully unit-testable.

import type { RepositorySummary } from '../../api/types';

/**
 * Human-facing family label. The backend naming convention suffixes every
 * split family with `-split` (`jeryu-split`, `veox-split`, ...); that suffix
 * is operational bookkeeping, not something operators want to read, so the UI
 * strips it everywhere a family name is DISPLAYED. Raw values keep flowing
 * through filters, URLs, and API queries untouched.
 */
export function formatFamilyName(family: string): string {
  return family.endsWith('-split') ? family.slice(0, -'-split'.length) : family;
}

export interface FamilyRollup {
  /** Family label as returned by the backend. */
  name: string;
  /** Member repositories, in the order they were received. */
  repos: RepositorySummary[];
  memberCount: number;
  /**
   * Worst-of member health. Canonical values (see RepoHealthPill):
   * `failing` > `degraded` > anything unknown > `healthy`.
   */
  health: string;
  openPullRequests: number;
  failingChecks: number;
  runningJobs: number;
  activeAgents: number;
  /** Max member `updated_at` (ISO timestamp). */
  updatedAt: string;
  /**
   * Worst (minimum) member `jankurai_score`; `null` when no member carries
   * a numeric score. Members without a score do not drag the rollup down —
   * "no audit yet" is not the same as a bad audit.
   */
  worstScore: number | null;
}

/**
 * Severity rank for worst-of health aggregation. Unknown values rank between
 * `healthy` and the canonical bad states so a family never looks healthier
 * than an unrecognised member, but an unknown label cannot mask a real
 * `degraded`/`failing` member either.
 */
function healthRank(health: string): number {
  switch (health) {
    case 'failing':
      return 3;
    case 'degraded':
      return 2;
    case 'healthy':
      return 0;
    default:
      return 1;
  }
}

function updatedAtMillis(repo: RepositorySummary): number {
  const ms = new Date(repo.updated_at).getTime();
  return Number.isFinite(ms) ? ms : Number.NEGATIVE_INFINITY;
}

export function aggregateFamily(
  name: string,
  repos: RepositorySummary[]
): FamilyRollup {
  let health = 'healthy';
  let worstRank = -1;
  let openPullRequests = 0;
  let failingChecks = 0;
  let runningJobs = 0;
  let activeAgents = 0;
  let updatedAt = '';
  let updatedAtMs = Number.NEGATIVE_INFINITY;
  let worstScore: number | null = null;

  for (const repo of repos) {
    const rank = healthRank(repo.health);
    if (rank > worstRank) {
      worstRank = rank;
      health = repo.health;
    }
    openPullRequests += repo.open_pull_requests;
    failingChecks += repo.failing_checks;
    runningJobs += repo.running_jobs;
    activeAgents += repo.active_agents;
    const score = repo.jankurai_score;
    if (typeof score === 'number' && (worstScore === null || score < worstScore)) {
      worstScore = score;
    }
    const ms = updatedAtMillis(repo);
    if (updatedAt === '' || ms > updatedAtMs) {
      updatedAtMs = ms;
      updatedAt = repo.updated_at;
    }
  }

  return {
    name,
    repos,
    memberCount: repos.length,
    health,
    openPullRequests,
    failingChecks,
    runningJobs,
    activeAgents,
    updatedAt,
    worstScore,
  };
}

export function partitionByFamily(repos: RepositorySummary[]): {
  families: FamilyRollup[];
  singles: RepositorySummary[];
} {
  const buckets = new Map<string, RepositorySummary[]>();
  const singles: RepositorySummary[] = [];
  for (const repo of repos) {
    if (repo.family) {
      const bucket = buckets.get(repo.family);
      if (bucket) {
        bucket.push(repo);
      } else {
        buckets.set(repo.family, [repo]);
      }
    } else {
      singles.push(repo);
    }
  }
  const families = [...buckets.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, members]) => aggregateFamily(name, members));
  return { families, singles };
}
