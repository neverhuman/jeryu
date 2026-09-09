// NotificationInbox.test.tsx — pure-function smoke for W-FE-18.
//
// Covers the three exported helpers that drive the UI:
//   * `filterViewerRelevantEvents` — keeps `user.{id}.notifications`,
//     `pull.*`, `repo.*`; drops everything else.
//   * `countUnread` — counts events after `lastSeen`; null lastSeen means
//     everything is unread.
//   * `groupNotificationsByDate` — buckets by today/yesterday/this-week/older.

import { describe, expect, it } from 'vitest';

import {
  countUnread,
  filterViewerRelevantEvents,
  groupNotificationsByDate,
} from '../NotificationInbox';
import type { WebEvent } from '../../api/types';

function mkEvent(
  partial: Partial<Omit<WebEvent, 'seq'>> & { seq?: number | bigint } & Pick<
    WebEvent,
    'scope'
  >
): WebEvent {
  const seq = partial.seq === undefined ? 1 : partial.seq;
  return {
    seq: typeof seq === 'bigint' ? seq : BigInt(seq),
    timestamp: partial.timestamp ?? '2026-05-26T12:00:00Z',
    scope: partial.scope,
    kind: partial.kind ?? 'pull.opened',
    entity: partial.entity ?? 'pull/1',
    summary: partial.summary ?? 'summary',
    payload: partial.payload ?? {},
  };
}

describe('filterViewerRelevantEvents', () => {
  it('includes user.<id>.notifications scope', () => {
    const events = [
      mkEvent({ scope: 'user.viewer-1.notifications' }),
      mkEvent({ scope: 'user.someone-else.notifications' }),
    ];
    const filtered = filterViewerRelevantEvents(events, 'viewer-1');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.scope).toBe('user.viewer-1.notifications');
  });

  it('includes pull.* and repo.* scopes', () => {
    const events = [
      mkEvent({ scope: 'pull.foo/bar/42' }),
      mkEvent({ scope: 'pull:foo/bar/42' }),
      mkEvent({ scope: 'repo.foo/bar' }),
      mkEvent({ scope: 'repo:foo/bar' }),
      mkEvent({ scope: 'system.health' }),
    ];
    const filtered = filterViewerRelevantEvents(events, null);
    expect(filtered).toHaveLength(4);
  });

  it('caps the list at 50 entries', () => {
    const events = Array.from({ length: 100 }, (_, i) =>
      mkEvent({ scope: 'pull.foo/bar/' + i, seq: i })
    );
    const filtered = filterViewerRelevantEvents(events, null);
    expect(filtered).toHaveLength(50);
  });
});

describe('countUnread', () => {
  it('treats null lastSeen as "everything unread"', () => {
    const events = [
      mkEvent({ scope: 'pull.foo', timestamp: '2026-05-26T12:00:00Z' }),
      mkEvent({ scope: 'pull.bar', timestamp: '2026-05-26T13:00:00Z' }),
    ];
    expect(countUnread(events, null)).toBe(2);
  });

  it('counts events newer than lastSeen', () => {
    const events = [
      mkEvent({ scope: 'pull.foo', timestamp: '2026-05-26T11:00:00Z' }),
      mkEvent({ scope: 'pull.bar', timestamp: '2026-05-26T13:00:00Z' }),
    ];
    expect(countUnread(events, '2026-05-26T12:00:00Z')).toBe(1);
  });

  it('returns 0 when every event is older than lastSeen', () => {
    const events = [
      mkEvent({ scope: 'pull.foo', timestamp: '2026-05-26T11:00:00Z' }),
    ];
    expect(countUnread(events, '2026-05-26T12:00:00Z')).toBe(0);
  });
});

describe('groupNotificationsByDate', () => {
  it('buckets into today / yesterday / this-week / older', () => {
    const now = new Date('2026-05-27T12:00:00Z');
    const events = [
      mkEvent({ scope: 'pull.a', timestamp: '2026-05-27T08:00:00Z' }), // today
      mkEvent({ scope: 'pull.b', timestamp: '2026-05-26T22:00:00Z' }), // yesterday
      mkEvent({ scope: 'pull.c', timestamp: '2026-05-23T12:00:00Z' }), // this-week
      mkEvent({ scope: 'pull.d', timestamp: '2026-05-10T12:00:00Z' }), // older
    ];
    const groups = groupNotificationsByDate(events, now);
    const keys = groups.map((g) => g.key);
    expect(keys).toEqual(['today', 'yesterday', 'this-week', 'older']);
    expect(groups[0]?.events).toHaveLength(1);
    expect(groups[1]?.events).toHaveLength(1);
    expect(groups[2]?.events).toHaveLength(1);
    expect(groups[3]?.events).toHaveLength(1);
  });

  it('omits empty groups', () => {
    const now = new Date('2026-05-27T12:00:00Z');
    const events = [
      mkEvent({ scope: 'pull.a', timestamp: '2026-05-27T08:00:00Z' }),
    ];
    const groups = groupNotificationsByDate(events, now);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.key).toBe('today');
  });
});
