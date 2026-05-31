// LeftNav.tsx — primary navigation (W-FE-01).

import { NavLink } from 'react-router-dom';
import {
  Bell,
  Cog,
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
  { to: '/fleet', label: 'Fleet', icon: ServerCog },
  { to: '/notifications', label: 'Notifications', icon: Bell },
  { to: '/audit', label: 'Audit', icon: ShieldCheck },
  { to: '/settings', label: 'Settings', icon: Cog },
];

export function LeftNav(): JSX.Element {
  return (
    <nav className="left-nav" aria-label="Primary">
      <span className="left-nav__group">Workspace</span>
      {NAV_ITEMS.map((item) => (
        <NavLink
          key={item.to}
          to={item.to}
          end={item.end}
          className={({ isActive }) =>
            `left-nav__item${isActive ? ' is-active' : ''}`
          }
        >
          <item.icon aria-hidden="true" size={16} />
          {item.label}
        </NavLink>
      ))}
      <div className="left-nav__divider" />
      <span className="left-nav__group">Activity</span>
      <NavLink
        to="/audit"
        className={({ isActive }) =>
          `left-nav__item${isActive ? ' is-active' : ''}`
        }
      >
        <History aria-hidden="true" size={16} />
        Recent events
      </NavLink>
    </nav>
  );
}
