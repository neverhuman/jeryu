// LeftNav.tsx — primary navigation (W-FE-01).

import { useLocation } from 'react-router-dom';
import {
  Bell,
  Cog,
  Brain,
  FolderGit2,
  GitMerge,
  History,
  LayoutDashboard,
  ServerCog,
  ShieldCheck,
  type LucideIcon,
} from 'lucide-react';

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  end?: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { to: '/', label: 'Dashboard', icon: LayoutDashboard, end: true },
  { to: '/repos', label: 'Repositories', icon: FolderGit2 },
  // Reconciled: the route map (router.tsx) and command palette both use
  // `/pull-room` / "Pull Room"; the nav previously pointed at a dead
  // `/merge-room` path that fell through to NotFound.
  { to: '/pull-room', label: 'Pull Room', icon: GitMerge },
  { to: '/intelligence', label: 'Intelligence', icon: Brain },
  { to: '/fleet', label: 'Fleet', icon: ServerCog },
  { to: '/notifications', label: 'Notifications', icon: Bell },
  { to: '/audit', label: 'Audit', icon: ShieldCheck },
  { to: '/settings', label: 'Settings', icon: Cog },
];

export function LeftNav(): JSX.Element {
  const { pathname } = useLocation();

  return (
    <nav className="left-nav" aria-label="Primary">
      <span className="left-nav__group">Workspace</span>
      {NAV_ITEMS.map((item) => (
        <a
          key={item.to}
          href={item.to}
          className={`left-nav__item${
            isActivePath(pathname, item.to, item.end) ? ' is-active' : ''
          }`}
          aria-current={isActivePath(pathname, item.to, item.end) ? 'page' : undefined}
        >
          <item.icon aria-hidden="true" size={16} />
          {item.label}
        </a>
      ))}
      <div className="left-nav__divider" />
      <span className="left-nav__group">Activity</span>
      <a
        href="/audit"
        className={`left-nav__item${
          isActivePath(pathname, '/audit') ? ' is-active' : ''
        }`}
        aria-current={isActivePath(pathname, '/audit') ? 'page' : undefined}
      >
        <History aria-hidden="true" size={16} />
        Recent events
      </a>
    </nav>
  );
}

function isActivePath(pathname: string, to: string, end = false): boolean {
  if (end || to === '/') {
    return pathname === to;
  }
  return pathname === to || pathname.startsWith(`${to}/`);
}
