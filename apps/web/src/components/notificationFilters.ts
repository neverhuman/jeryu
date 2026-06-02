// notificationFilters.ts — pure selectors for the notification inbox.
//
// These functions derive the viewer-relevant subset of the realtime event
// buffer, count unread events against the last-seen stamp, and bucket events
// by date. They are pure (no React, no DOM) so they unit-test cleanly and
// are shared by the header popover (`NotificationInbox`) and the dedicated
// `/notifications` page.

import type { WebEvent } from '../api/types';

const MAX_EVENTS = 50;

export function filterViewerRelevantEvents(
  events: WebEvent[],
  viewerId: string | null
): WebEvent[] {
  const userScope = viewerId ? `user.${viewerId}.notifications` : null;
  const filtered: WebEvent[] = [];
  for (const event of events) {
    if (isViewerRelevantScope(event.scope, userScope)) {
      filtered.push(event);
    }
  }
  return filtered.slice(0, MAX_EVENTS);
}

function isViewerRelevantScope(
  scope: string,
  userScope: string | null
): boolean {
  if (!scope) return false;
  if (userScope && scope === userScope) return true;
  if (scope.startsWith('pull.') || scope.startsWith('pull:')) return true;
  if (scope.startsWith('repo.') || scope.startsWith('repo:')) return true;
  return false;
}

export function countUnread(
  events: WebEvent[],
  lastSeen: string | null
): number {
  if (events.length === 0) return 0;
  if (!lastSeen) return events.length;
  const seenMs = Date.parse(lastSeen);
  if (!Number.isFinite(seenMs)) return events.length;
  let unread = 0;
  for (const event of events) {
    const eventMs = Date.parse(event.timestamp);
    if (Number.isFinite(eventMs) && eventMs > seenMs) {
      unread += 1;
    }
  }
  return unread;
}

export interface NotificationGroup {
  key: 'today' | 'yesterday' | 'this-week' | 'older';
  label: string;
  events: WebEvent[];
}

export function groupNotificationsByDate(
  events: WebEvent[],
  now: Date = new Date()
): NotificationGroup[] {
  const buckets: Record<NotificationGroup['key'], WebEvent[]> = {
    today: [],
    yesterday: [],
    'this-week': [],
    older: [],
  };
  // Use midnight boundaries so "today" = same calendar day, "yesterday"
  // = previous calendar day, "this week" = within 7 days, else "older".
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const midnightMs = midnight.getTime();
  const dayMs = 24 * 60 * 60 * 1000;
  for (const event of events) {
    const eventMs = Date.parse(event.timestamp);
    if (!Number.isFinite(eventMs)) {
      buckets.older.push(event);
      continue;
    }
    if (eventMs >= midnightMs) {
      buckets.today.push(event);
    } else if (eventMs >= midnightMs - dayMs) {
      buckets.yesterday.push(event);
    } else if (eventMs >= midnightMs - 6 * dayMs) {
      buckets['this-week'].push(event);
    } else {
      buckets.older.push(event);
    }
  }
  const groups: NotificationGroup[] = [
    { key: 'today', label: 'Today', events: buckets.today },
    { key: 'yesterday', label: 'Yesterday', events: buckets.yesterday },
    { key: 'this-week', label: 'This week', events: buckets['this-week'] },
    { key: 'older', label: 'Older', events: buckets.older },
  ];
  return groups.filter((g) => g.events.length > 0);
}
