// SettingsLayout.tsx — left-nav + main content shell (W-FE-12).
//
// Sections come from §7.4 W-FE-12: general, features, merge policy, branch
// protection, ci, agents, access, security, notifications, retention, danger
// zone. The active section is driven by the URL so the layout itself is
// stateless — the parent page wires `useParams().section` to the active id.

import {
  AlertOctagon,
  Bell,
  Bot,
  Clock4,
  Cog,
  GitBranch,
  GitMerge,
  Globe,
  KeyRound,
  Lock,
  Sparkles,
  type LucideIcon,
} from 'lucide-react';
import type { ReactNode } from 'react';

import './settings.css';

export interface SettingsNavItem {
  id: string;
  label: string;
  icon: LucideIcon;
  /** When true the item is rendered with the danger-zone treatment. */
  danger?: boolean;
}

export const SETTINGS_NAV: SettingsNavItem[] = [
  { id: 'general', label: 'General', icon: Cog },
  { id: 'features', label: 'Features', icon: Sparkles },
  { id: 'merge-policy', label: 'Merge policy', icon: GitMerge },
  { id: 'branch-protection', label: 'Branch protection', icon: GitBranch },
  { id: 'ci', label: 'CI / Workflows', icon: Globe },
  { id: 'agents', label: 'Agents', icon: Bot },
  { id: 'access', label: 'Access', icon: KeyRound },
  { id: 'security', label: 'Security', icon: Lock },
  { id: 'notifications', label: 'Notifications', icon: Bell },
  { id: 'retention', label: 'Retention', icon: Clock4 },
  { id: 'danger-zone', label: 'Danger zone', icon: AlertOctagon, danger: true },
];

export interface SettingsLayoutProps {
  activeSection: string;
  /** Builds the URL for a section. Allows the parent to keep router concerns. */
  hrefFor: (section: string) => string;
  /** Render-prop link wrapper (React Router `<Link>`). */
  renderLink: (props: {
    section: SettingsNavItem;
    href: string;
    active: boolean;
    children: ReactNode;
  }) => ReactNode;
  children: ReactNode;
  className?: string;
}

export function SettingsLayout({
  activeSection,
  hrefFor,
  renderLink,
  children,
  className,
}: SettingsLayoutProps): JSX.Element {
  return (
    <div
      className={`settings-layout ${className ?? ''}`.trim()}
      data-section={activeSection}
    >
      <nav className="settings-layout__nav" aria-label="Repository settings">
        <ul className="settings-layout__nav-list">
          {SETTINGS_NAV.map((section) => {
            const Icon = section.icon;
            const active = section.id === activeSection;
            const href = hrefFor(section.id);
            const content = (
              <>
                <Icon aria-hidden="true" size={14} />
                <span>{section.label}</span>
              </>
            );
            return (
              <li
                key={section.id}
                className={`settings-layout__nav-item ${
                  section.danger ? 'settings-layout__nav-item--danger' : ''
                } ${active ? 'settings-layout__nav-item--active' : ''}`.trim()}
              >
                {renderLink({ section, href, active, children: content })}
              </li>
            );
          })}
        </ul>
      </nav>
      <section className="settings-layout__main">{children}</section>
    </div>
  );
}
