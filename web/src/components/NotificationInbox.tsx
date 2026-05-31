// NotificationInbox.tsx — header popover (W-FE-18).
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
// The pure selectors (`filterViewerRelevantEvents`, `countUnread`,
// `groupNotificationsByDate`) live in `./notificationFilters`, and the
// shared `<NotificationListView>` lives in `./NotificationListView`; both
// are re-exported here so existing import sites keep working unchanged.

import { Bell, Check, ExternalLink } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link } from 'react-router-dom';

import { useRealtimeStore } from '../stores/realtimeStore';
import { usePreferencesStore } from '../stores/preferencesStore';
import {
  countUnread,
  filterViewerRelevantEvents,
} from './notificationFilters';
import { NotificationListView } from './NotificationListView';

import './NotificationInbox.css';

export {
  countUnread,
  filterViewerRelevantEvents,
  groupNotificationsByDate,
} from './notificationFilters';
export type { NotificationGroup } from './notificationFilters';
export { NotificationListView } from './NotificationListView';
export type { NotificationListViewProps } from './NotificationListView';

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
