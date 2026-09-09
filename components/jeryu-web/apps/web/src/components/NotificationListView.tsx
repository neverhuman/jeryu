// NotificationListView.tsx — grouped list rendering for the inbox (W-FE-18).
//
// Shared between the header popover (`NotificationInbox`) and the dedicated
// `/notifications` page so grouping, empty state, and per-item link logic
// stay in one place. Read-only: there is no per-item "mark as read"; the
// unread highlight is derived from each event's timestamp vs. `lastSeen`.

import { Bell } from 'lucide-react';
import { useMemo } from 'react';
import { Link } from 'react-router-dom';

import type { WebEvent } from '../api/types';
import { groupNotificationsByDate } from './notificationFilters';

import './NotificationInbox.css';

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

function scopeToHref(scope: string): string {
  // Phase-1 mapping: `repo:owner/name` -> `/repos/jeryu/owner/name`,
  // `pull:owner/name/number` -> the PR page. The provider segment is
  // host-neutral; the SPA renders it as `jeryu` because the forge is its
  // own host. v1.5 can route on the actual provider once the bus tags it.
  const colon = scope.indexOf(':');
  if (colon === -1) return '';
  const kind = scope.slice(0, colon);
  const rest = scope.slice(colon + 1);
  if (!rest) return '';
  if (kind === 'repo') {
    return `/repos/jeryu/${rest}`;
  }
  if (kind === 'pull') {
    const lastSlash = rest.lastIndexOf('/');
    if (lastSlash === -1) return '';
    const repo = rest.slice(0, lastSlash);
    const prNumber = rest.slice(lastSlash + 1);
    return `/repos/jeryu/${repo}/pulls/${prNumber}`;
  }
  return '';
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
