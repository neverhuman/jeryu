// NotificationInbox.tsx — header popover + shared list view (W-FE-18).
//
// Reads the rolling event buffer from `realtimeStore` and filters down to
// the viewer-relevant subset:
//   * Any event with scope matching `user.{viewerId}.notifications`.
//   * Any event whose scope is namespaced under `pull.*` or `repo.*` — both
//     surface to the viewer because they drive the dashboard cards. v1
//     errs on the side of including too much rather than too little; the
//     v1.5 granular rules can narrow this once preferences land.
//
// The popover is read-only: there is no per-event "mark as read"; instead
// "Mark all as read" stamps `preferencesStore.notificationsLastSeen` to the
// current timestamp. Unread count is computed live from events whose
// `timestamp` is strictly greater than the stored stamp.
//
// The same `<NotificationListView>` is consumed by `NotificationsPage`
// (full route) so the popover and the dedicated page share grouping/empty
// state logic.

import { Bell, Check, ExternalLink } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link } from 'react-router-dom';

import type { WebEvent } from '../api/types';
import { useRealtimeStore } from '../stores/realtimeStore';
import { usePreferencesStore } from '../stores/preferencesStore';

import './NotificationInbox.css';

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

function NotificationItem({
  event,
  isUnread,
}: {
  event: WebEvent;
  isUnread: boolean;
}): JSX.Element {
  // Best-effort target route from scope. `repo:owner/name` and
  // `pull:owner/name/number` are the common shapes; the SPA renders the
  // human URL via `repos/:provider/:fullName/*`. We fall back to a
  // dead anchor (`#`) for unknown scopes so links remain visible but
  // non-navigational.
  const href = scopeToHref(event.scope);
  const title = `${event.kind} · ${event.entity}`;
  const time = formatTime(event.timestamp);
  return (
    <li
      className={`notification-inbox__item${
        isUnread ? ' notification-inbox__item--unread' : ''
      }`}
      data-unread={isUnread ? 'true' : undefined}
    >
      {href ? (
        <Link to={href} className="notification-inbox__item-link">
          <span className="notification-inbox__item-title">{title}</span>
          <span className="notification-inbox__item-summary">
            {event.summary}
          </span>
          <span className="notification-inbox__item-meta">
            <span className="notification-inbox__item-scope">{event.scope}</span>
            <span className="notification-inbox__item-time">{time}</span>
          </span>
        </Link>
      ) : (
        <div className="notification-inbox__item-link notification-inbox__item-link--static">
          <span className="notification-inbox__item-title">{title}</span>
          <span className="notification-inbox__item-summary">
            {event.summary}
          </span>
          <span className="notification-inbox__item-meta">
            <span className="notification-inbox__item-scope">{event.scope}</span>
            <span className="notification-inbox__item-time">{time}</span>
          </span>
        </div>
      )}
    </li>
  );
}

function formatTime(iso: string): string {
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return iso;
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return iso;
  }
}

function scopeToHref(scope: string): string | null {
  // Phase-1 mapping: `repo:owner/name` -> `/repos/jeryu/owner/name`,
  // `pull:owner/name/number` -> the PR page. The provider segment is
  // host-neutral; the SPA renders it as `jeryu` because the forge is its
  // own host. v1.5 can route on the actual provider once the bus tags it.
  const colon = scope.indexOf(':');
  if (colon === -1) return null;
  const kind = scope.slice(0, colon);
  const rest = scope.slice(colon + 1);
  if (!rest) return null;
  if (kind === 'repo') {
    return `/repos/jeryu/${rest}`;
  }
  if (kind === 'pull') {
    const lastSlash = rest.lastIndexOf('/');
    if (lastSlash === -1) return null;
    const repo = rest.slice(0, lastSlash);
    const prNumber = rest.slice(lastSlash + 1);
    return `/repos/jeryu/${repo}/pulls/${prNumber}`;
  }
  return null;
}

export interface NotificationListViewProps {
  events: WebEvent[];
  lastSeen: string | null;
  /** Render larger headings on the dedicated /notifications page. */
  variant?: 'popover' | 'page';
  className?: string;
}

export function NotificationListView({
  events,
  lastSeen,
  variant = 'popover',
  className,
}: NotificationListViewProps): JSX.Element {
  const groups = useMemo(() => groupNotificationsByDate(events), [events]);
  const lastSeenMs = lastSeen ? Date.parse(lastSeen) : NaN;

  if (events.length === 0) {
    return (
      <div
        className={`notification-inbox__empty ${className ?? ''}`.trim()}
        role="status"
      >
        <span className="notification-inbox__empty-icon" aria-hidden="true">
          <Bell size={20} />
        </span>
        <p className="notification-inbox__empty-title">No notifications yet.</p>
        <p className="notification-inbox__empty-hint">
          Mentions, review requests, and merge events for repositories you
          watch will appear here.
        </p>
      </div>
    );
  }

  return (
    <div
      className={`notification-inbox__list-view notification-inbox__list-view--${variant} ${
        className ?? ''
      }`.trim()}
    >
      {groups.map((group) => (
        <section
          key={group.key}
          className="notification-inbox__group"
          aria-label={`${group.label} notifications`}
        >
          <h3 className="notification-inbox__group-title">{group.label}</h3>
          <ul className="notification-inbox__group-list">
            {group.events.map((event) => {
              const eventMs = Date.parse(event.timestamp);
              const isUnread =
                Number.isFinite(eventMs) &&
                Number.isFinite(lastSeenMs) &&
                eventMs > lastSeenMs;
              return (
                <NotificationItem
                  key={`${String(event.seq)}-${event.scope}`}
                  event={event}
                  isUnread={isUnread || !Number.isFinite(lastSeenMs)}
                />
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}

export interface NotificationInboxProps {
  viewerId: string | null;
}

export function NotificationInbox({
  viewerId,
}: NotificationInboxProps): JSX.Element {
  const events = useRealtimeStore((s) => s.events);
  const lastSeen = usePreferencesStore((s) => s.notificationsLastSeen);
  const markSeen = usePreferencesStore((s) => s.markNotificationsSeen);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const filteredEvents = useMemo(
    () => filterViewerRelevantEvents(events, viewerId),
    [events, viewerId]
  );
  const unread = useMemo(
    () => countUnread(filteredEvents, lastSeen),
    [filteredEvents, lastSeen]
  );

  // Close on outside click / Escape so the popover behaves like a menu.
  useEffect(() => {
    if (!open) return undefined;
    const onDown = (event: MouseEvent): void => {
      if (!containerRef.current) return;
      if (!containerRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        setOpen(false);
      }
    };
    window.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const handleMarkAll = useCallback(() => {
    markSeen();
  }, [markSeen]);

  return (
    <div ref={containerRef} className="notification-inbox">
      <button
        type="button"
        className="notification-inbox__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={
          unread > 0
            ? `Notifications (${unread} unread)`
            : 'Notifications (none unread)'
        }
        onClick={() => setOpen((prev) => !prev)}
      >
        <Bell size={16} aria-hidden="true" />
        {unread > 0 ? (
          <span
            className="notification-inbox__badge"
            aria-hidden="true"
            data-testid="notification-badge"
          >
            {unread > 99 ? '99+' : unread}
          </span>
        ) : null}
      </button>
      {open ? (
        <div
          className="notification-inbox__popover"
          role="dialog"
          aria-label="Notifications"
        >
          <header className="notification-inbox__popover-header">
            <h2 className="notification-inbox__popover-title">Notifications</h2>
            <div className="notification-inbox__popover-actions">
              <button
                type="button"
                className="notification-inbox__mark-all"
                onClick={handleMarkAll}
                disabled={unread === 0}
              >
                <Check size={12} aria-hidden="true" />
                Mark all as read
              </button>
              <Link
                to="/notifications"
                className="notification-inbox__open-full"
                onClick={() => setOpen(false)}
              >
                <ExternalLink size={12} aria-hidden="true" />
                View all
              </Link>
            </div>
          </header>
          <NotificationListView
            events={filteredEvents}
            lastSeen={lastSeen}
            variant="popover"
          />
        </div>
      ) : null}
    </div>
  );
}
