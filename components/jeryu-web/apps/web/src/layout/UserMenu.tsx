// UserMenu.tsx — top-right identity status and logout control (W-FE-01).

import { LogOut } from 'lucide-react';

import { useAuth } from '../hooks/useAuth';

interface UserMenuProps {
  login: string;
  displayName: string | null;
}

export function UserMenu({ login, displayName }: UserMenuProps): JSX.Element {
  const label = displayName ?? login;
  const { logout } = useAuth();
  return (
    <div className="global-header__account">
      <span className="global-header__user" aria-label={`Logged in as ${label}`}>
        <span className="global-header__user-prefix">Logged in</span>
        <span aria-hidden="true">·</span>
        <span className="global-header__user-name">{label}</span>
      </span>
      <button
        type="button"
        className="global-header__logout"
        aria-label="Log out"
        title="Log out"
        onClick={() => logout.mutate()}
      >
        <LogOut size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
