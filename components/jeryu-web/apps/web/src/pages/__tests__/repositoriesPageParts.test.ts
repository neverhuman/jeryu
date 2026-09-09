import { describe, expect, it } from 'vitest';

import type { RepositorySummary } from '../../api/types';
import { groupByFamily } from '../repositoriesPageParts';

function repo(name: string, role: RepositorySummary['repo_role']): RepositorySummary {
  return {
    id: {
      id: `repo-${name}`,
      host: 'jeryu',
      owner: 'neverhuman',
      name,
    },
    entity: { kind: 'repository', id: `repo-${name}` },
    description: null,
    visibility: 'public',
    default_branch: 'main',
    family: 'jeryu-split',
    repo_role: role,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 0,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-05-26T12:00:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
  };
}

describe('groupByFamily', () => {
  it('sorts the jeryu public portal first within the split family', () => {
    const groups = groupByFamily([
      repo('jeryu-core', 'split_member'),
      repo('jeryu', 'public_portal'),
    ]);

    expect(groups[0].title).toBe('jeryu');
    expect(groups[0].repos.map((item) => item.id.name)).toEqual([
      'jeryu',
      'jeryu-core',
    ]);
  });
});
