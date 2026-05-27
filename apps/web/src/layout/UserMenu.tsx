// UserMenu.tsx — top-right identity chip (W-FE-01).
//
// Phase 1 renders an inline chip; future work adds a dropdown for theme,
// sign-out, profile.

import { User } from 'lucide-react';

interface UserMenuProps {
  login: string;
  displayName: string | null;
}

export function UserMenu({ login, displayName }: UserMenuProps): JSX.Element {
  const label = displayName ?? login;
  return (
    <button
      type="button"
      className="global-header__user"
      aria-label={`Account menu for ${label}`}
    >
      <User size={14} aria-hidden="true" />
      <span className="global-header__user-name">{label}</span>
    </button>
  );
}
