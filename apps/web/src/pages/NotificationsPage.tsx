// NotificationsPage.tsx — full notifications inbox view (W-FE-18).
//
// Sister surface to the header popover (`NotificationInbox`). Renders the
// last 50 viewer-relevant events from `realtimeStore`, grouped by date
// (today / yesterday / this week / older). The mark-all-as-read action is
// the same store mutation the popover uses; both surfaces stay in sync
// because they read the same Zustand state.

import { Check } from 'lucide-react';
import { useCallback, useMemo } from 'react';

import {
  NotificationListView,
  countUnread,
  filterViewerRelevantEvents,
} from '../components/NotificationInbox';
import { useBootstrap } from '../hooks/useBootstrap';
import { useRealtimeStore } from '../stores/realtimeStore';
import { usePreferencesStore } from '../stores/preferencesStore';

import './page.css';

export function NotificationsPage(): JSX.Element {
  const bootstrap = useBootstrap();
  const events = useRealtimeStore((s) => s.events);
  const lastSeen = usePreferencesStore((s) => s.notificationsLastSeen);
  const markSeen = usePreferencesStore((s) => s.markNotificationsSeen);

  const viewerId = bootstrap.data?.viewer.id ?? null;
  const filteredEvents = useMemo(
    () => filterViewerRelevantEvents(events, viewerId),
    [events, viewerId]
  );
  const unread = useMemo(
    () => countUnread(filteredEvents, lastSeen),
    [filteredEvents, lastSeen]
  );

  const handleMarkAll = useCallback(() => {
    markSeen();
  }, [markSeen]);

  return (
    <div className="page" data-testid="notifications-page">
      <header className="page__header">
        <h1 className="page__title">Notifications</h1>
        <p className="page__subtitle">
          The last 50 events scoped to you — mentions, pull requests, repo
          updates. Streams live from the activity bus.
        </p>
        <div className="page__inline-actions">
          <span
            className={`page__pill ${
              unread > 0 ? 'page__pill--warning' : 'page__pill--success'
            }`}
          >
            {unread > 0 ? `${unread} unread` : 'All caught up'}
          </span>
          <button
            type="button"
            className="notification-inbox__mark-all"
            onClick={handleMarkAll}
            disabled={unread === 0}
            aria-label="Mark all notifications as read"
          >
            <Check size={12} aria-hidden="true" />
            Mark all as read
          </button>
        </div>
      </header>
      <NotificationListView
        events={filteredEvents}
        lastSeen={lastSeen}
        variant="page"
      />
    </div>
  );
}
