// UserMenu.tsx — top-right identity chip (W-FE-01).
//
// Renders an inline identity chip. The theme / sign-out / profile dropdown
// is layered on by a later work package.

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
