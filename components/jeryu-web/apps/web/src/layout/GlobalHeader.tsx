// GlobalHeader.tsx — top bar (W-FE-01).
//
// Owns: brand mark, command palette trigger, repo switcher, live status pill,
// user menu. Each child is its own file so the shell layout stays scannable.

import { Search } from 'lucide-react';

import { JeryuLogo } from '../components/brand/JeryuLogo';
import { NotificationInbox } from '../components/NotificationInbox';
import { useBootstrap } from '../hooks/useBootstrap';
import { useRealtimeStore } from '../stores/realtimeStore';
import { useCommandStore } from '../stores/commandStore';
import { RepoSwitcher } from './RepoSwitcher';
import { UserMenu } from './UserMenu';

export function GlobalHeader(): JSX.Element {
  const openPalette = useCommandStore((s) => s.open);
  const status = useRealtimeStore((s) => s.status);
  const bootstrap = useBootstrap();

  const liveLabel = liveLabelFor(status);
  const platformHint = navigator.userAgent.includes('Mac') ? '⌘ K' : 'Ctrl K';

  return (
    <div className="global-header">
      <span className="global-header__brand">
        <JeryuLogo variant="header" />
      </span>
      <RepoSwitcher />
      <span className="global-header__spacer" aria-hidden="true" />
      <button
        type="button"
        className="global-header__cmdk"
        onClick={() => openPalette()}
        aria-label="Open command palette"
      >
        <Search size={14} aria-hidden="true" />
        Quick actions, commands…
        <span className="global-header__cmdk-hint" aria-hidden="true">
          {platformHint}
        </span>
      </button>
      <span
        className={`global-header__live global-header__live--${status}`}
        aria-live="polite"
        aria-atomic="true"
      >
        <span
          className="global-header__live-dot"
          aria-hidden="true"
        />
        {liveLabel}
      </span>
      <NotificationInbox viewerId={bootstrap.data?.viewer.id ?? null} />
      <UserMenu
        login={bootstrap.data?.viewer.login ?? 'Loading…'}
        displayName={bootstrap.data?.viewer.display_name ?? null}
      />
    </div>
  );
}

function liveLabelFor(status: string): string {
  switch (status) {
    case 'open':
      return 'Live';
    case 'connecting':
      return 'Connecting…';
    case 'reconnecting':
      return 'Reconnecting…';
    case 'closed':
      return 'Offline';
    default:
      return 'Idle';
  }
}
