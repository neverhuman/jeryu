// familyRollup.test.ts — unit tests for the pure family aggregation helpers.

import { describe, expect, it } from 'vitest';

import {
  aggregateFamily,
  formatFamilyName,
  partitionByFamily,
} from '../familyRollup';
import type { RepositorySummary } from '../../../api/types';

let nextId = 0;

function repo(overrides: Partial<RepositorySummary> = {}): RepositorySummary {
  nextId += 1;
  const name = `repo-${nextId}`;
  return {
    id: { id: `uuid-${nextId}`, host: 'jeryu', owner: 'veox', name },
    entity: { kind: 'repository', id: `uuid-${nextId}` },
    description: null,
    visibility: 'internal',
    default_branch: 'main',
    family: null,
    repo_role: null,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 0,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-06-01T00:00:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
    ...overrides,
  };
}

describe('aggregateFamily', () => {
  it('derives worst-of health: failing beats everything', () => {
    const rollup = aggregateFamily('fam', [
      repo({ health: 'healthy' }),
      repo({ health: 'failing' }),
      repo({ health: 'degraded' }),
    ]);
    expect(rollup.health).toBe('failing');
  });

  it('derives worst-of health: degraded beats healthy and unknown', () => {
    const rollup = aggregateFamily('fam', [
      repo({ health: 'healthy' }),
      repo({ health: 'archived' }),
      repo({ health: 'degraded' }),
    ]);
    expect(rollup.health).toBe('degraded');
  });

  it('ranks unknown health values above healthy', () => {
    const rollup = aggregateFamily('fam', [
      repo({ health: 'healthy' }),
      repo({ health: 'archived' }),
    ]);
    expect(rollup.health).toBe('archived');
  });

  it('stays healthy when every member is healthy', () => {
    const rollup = aggregateFamily('fam', [
      repo({ health: 'healthy' }),
      repo({ health: 'healthy' }),
    ]);
    expect(rollup.health).toBe('healthy');
  });

  it('sums activity counters and counts members', () => {
    const rollup = aggregateFamily('fam', [
      repo({
        open_pull_requests: 2,
        failing_checks: 1,
        running_jobs: 3,
        active_agents: 1,
      }),
      repo({
        open_pull_requests: 4,
        failing_checks: 0,
        running_jobs: 2,
        active_agents: 2,
      }),
    ]);
    expect(rollup.memberCount).toBe(2);
    expect(rollup.openPullRequests).toBe(6);
    expect(rollup.failingChecks).toBe(1);
    expect(rollup.runningJobs).toBe(5);
    expect(rollup.activeAgents).toBe(3);
  });

  it('takes the worst (minimum) member jankurai score', () => {
    const rollup = aggregateFamily('fam', [
      repo({ jankurai_score: 92 }),
      repo({ jankurai_score: 61 }),
      repo({ jankurai_score: 88 }),
    ]);
    expect(rollup.worstScore).toBe(61);
  });

  it('ignores members without a score when deriving worstScore', () => {
    const rollup = aggregateFamily('fam', [
      repo({ jankurai_score: 90 }),
      repo({ jankurai_score: null }),
      repo({}),
    ]);
    expect(rollup.worstScore).toBe(90);
  });

  it('worstScore is null when no member carries a score', () => {
    const rollup = aggregateFamily('fam', [repo(), repo()]);
    expect(rollup.worstScore).toBeNull();
  });

  it('takes the max updated_at', () => {
    const rollup = aggregateFamily('fam', [
      repo({ updated_at: '2026-06-02T10:00:00Z' }),
      repo({ updated_at: '2026-06-09T08:30:00Z' }),
      repo({ updated_at: '2026-05-30T23:59:59Z' }),
    ]);
    expect(rollup.updatedAt).toBe('2026-06-09T08:30:00Z');
  });

  it('keeps the family name and member list', () => {
    const members = [repo(), repo()];
    const rollup = aggregateFamily('jmcp-split', members);
    expect(rollup.name).toBe('jmcp-split');
    expect(rollup.repos).toEqual(members);
  });
});

describe('partitionByFamily', () => {
  it('routes repos without a family to singles', () => {
    const single = repo({ family: null });
    const member = repo({ family: 'veox-split' });
    const { families, singles } = partitionByFamily([single, member]);
    expect(singles).toEqual([single]);
    expect(families).toHaveLength(1);
    expect(families[0]?.name).toBe('veox-split');
    expect(families[0]?.repos).toEqual([member]);
  });

  it('sorts families by name', () => {
    const { families } = partitionByFamily([
      repo({ family: 'veox-split' }),
      repo({ family: 'jmcp-split' }),
      repo({ family: 'veox-split' }),
      repo({ family: 'aaa' }),
    ]);
    expect(families.map((f) => f.name)).toEqual([
      'aaa',
      'jmcp-split',
      'veox-split',
    ]);
    expect(families[2]?.memberCount).toBe(2);
  });

  it('handles null-only data (no families at all)', () => {
    const repos = [repo(), repo(), repo()];
    const { families, singles } = partitionByFamily(repos);
    expect(families).toEqual([]);
    expect(singles).toEqual(repos);
  });

  it('handles an empty list', () => {
    const { families, singles } = partitionByFamily([]);
    expect(families).toEqual([]);
    expect(singles).toEqual([]);
  });
});

describe('formatFamilyName', () => {
  it("strips the operational '-split' suffix for display", () => {
    expect(formatFamilyName('jeryu-split')).toBe('jeryu');
    expect(formatFamilyName('veox-split')).toBe('veox');
  });

  it('leaves non-convention names untouched', () => {
    expect(formatFamilyName('redline')).toBe('redline');
    expect(formatFamilyName('splitter')).toBe('splitter');
    expect(formatFamilyName('-split')).toBe('');
  });
});
