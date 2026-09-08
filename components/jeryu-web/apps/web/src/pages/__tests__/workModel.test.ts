import { describe, expect, it } from 'vitest';

import type { WorkItem } from '../../api/types';
import {
  DEFAULT_WORK_FILTERS,
  assigneeOptions,
  filterWorkItems,
  groupWorkItems,
  labelOptions,
  principalsFromInput,
  repoOptions,
} from '../workModel';

describe('workModel', () => {
  it('groups work items by status lanes', () => {
    const lanes = groupWorkItems([
      work({ key: 'JRY-1', status: 'ready' }),
      work({ key: 'JRY-2', status: 'blocked' }),
      work({ key: 'JRY-3', status: 'done' }),
    ]);

    expect(lanes.find((lane) => lane.id === 'ready')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'blocked')?.items).toHaveLength(1);
    expect(lanes.find((lane) => lane.id === 'done')?.items).toHaveLength(1);
  });

  it('filters by repo, kind, priority, assignee, label, and text', () => {
    const items = [
      work({
        title: 'Fix cache key',
        kind: 'bug',
        priority: 'p1',
        labels: ['cache'],
        assignees: [{ kind: 'human', id: 'alice', display_name: null }],
      }),
      work({
        key: 'JRY-2',
        title: 'Document runner setup',
        repo: { id: 'repo-2', host: 'jeryu', owner: 'bob', name: 'runner' },
        kind: 'docs',
        priority: 'p3',
        labels: ['docs'],
      }),
    ];

    expect(
      filterWorkItems(items, {
        ...DEFAULT_WORK_FILTERS,
        repo: 'alice/jeryu',
        kind: 'bug',
        priority: 'p1',
        assignee: 'alice',
        label: 'cache',
        search: 'cache',
      }).map((item) => item.key)
    ).toEqual(['JRY-1']);
  });

  it('builds stable option lists and agent principal tokens', () => {
    const items = [
      work({ labels: ['bug', 'cache'] }),
      work({
        key: 'JRY-2',
        repo: null,
        labels: ['cache'],
        assignees: [{ kind: 'agent', id: 'runner', display_name: null }],
      }),
    ];

    expect(repoOptions(items)).toEqual(['Unscoped', 'alice/jeryu']);
    expect(labelOptions(items)).toEqual(['bug', 'cache']);
    expect(assigneeOptions(items).map((assignee) => assignee.id)).toEqual([
      'runner',
    ]);
    expect(principalsFromInput('alice, agent:runner')).toEqual([
      { kind: 'human', id: 'alice', display_name: null },
      { kind: 'agent', id: 'runner', display_name: null },
    ]);
  });
});

function work(overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    id: overrides.id ?? `id-${overrides.key ?? 'JRY-1'}`,
    key: overrides.key ?? 'JRY-1',
    number: overrides.number ?? 1,
    repo:
      overrides.repo === undefined
        ? { id: 'repo-1', host: 'jeryu', owner: 'alice', name: 'jeryu' }
        : overrides.repo,
    title: overrides.title ?? 'Work item',
    body: overrides.body ?? null,
    status: overrides.status ?? 'ready',
    kind: overrides.kind ?? 'task',
    priority: overrides.priority ?? 'p2',
    labels: overrides.labels ?? [],
    assignees: overrides.assignees ?? [],
    issue: overrides.issue ?? null,
    pull_requests: overrides.pull_requests ?? [],
    created_at: overrides.created_at ?? '2026-07-02T00:00:00Z',
    updated_at: overrides.updated_at ?? '2026-07-02T00:00:00Z',
  };
}
