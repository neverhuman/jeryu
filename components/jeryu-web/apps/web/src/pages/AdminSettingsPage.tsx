// AdminSettingsPage.tsx — admin preferences surface.
//
// Implements theme and density preferences plus account access controls wired
// through typed HTTP endpoints.

import { Moon, Monitor, Sun, ToggleRight } from 'lucide-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { apiGet, apiSend } from '../api/client';
import { endpoints } from '../api/endpoints';
import { ActionButton } from '../components/action/ActionButton';
import { ErrorState, LoadingState } from '../components/state';
import { useAuth } from '../hooks/useAuth';
import {
  usePreferencesStore,
  type DensityPreference,
  type ThemePreference,
} from '../stores/preferencesStore';

import './page.css';

export function AdminSettingsPage(): JSX.Element {
  const theme = usePreferencesStore((s) => s.theme);
  const setTheme = usePreferencesStore((s) => s.setTheme);
  const density = usePreferencesStore((s) => s.density);
  const setDensity = usePreferencesStore((s) => s.setDensity);
  const { user } = useAuth();

  return (
    <div className="page" data-testid="settings-page">
      <header className="page__header">
        <h1 className="page__title">Settings</h1>
        <p className="page__subtitle">
          Admin preferences and account access controls.
        </p>
        <div className="page__inline-actions">
          <span className="page__pill page__pill--warning">
            Theme + density preferences
          </span>
        </div>
      </header>

      <section className="page__section" aria-labelledby="theme-section">
        <h2 className="page__section-title" id="theme-section">
          Theme
        </h2>
        <div className="page__inline-actions" role="radiogroup">
          <ThemeButton
            value="system"
            current={theme}
            label="System"
            icon={<Monitor size={14} />}
            onSelect={setTheme}
          />
          <ThemeButton
            value="light"
            current={theme}
            label="Light"
            icon={<Sun size={14} />}
            onSelect={setTheme}
          />
          <ThemeButton
            value="dark"
            current={theme}
            label="Dark"
            icon={<Moon size={14} />}
            onSelect={setTheme}
          />
          <ThemeButton
            value="high-contrast"
            current={theme}
            label="High contrast"
            icon={<ToggleRight size={14} />}
            onSelect={setTheme}
          />
        </div>
      </section>

      <section className="page__section" aria-labelledby="density-section">
        <h2 className="page__section-title" id="density-section">
          Density
        </h2>
        <div className="page__inline-actions" role="radiogroup">
          {(
            ['comfortable', 'compact', 'ultra-compact'] as DensityPreference[]
          ).map((value) => (
            <ActionButton
              key={value}
              variant={density === value ? 'primary' : 'default'}
              role="radio"
              aria-checked={density === value}
              onClick={() => setDensity(value)}
            >
              {value}
            </ActionButton>
          ))}
        </div>
      </section>

      {user?.role === 'admin' ? <AdminAccessPanel /> : null}
    </div>
  );
}

interface AdminUser {
  login: string;
  role: 'admin' | 'user';
  created_at: string;
  updated_at: string;
}

interface ResetReceipt {
  login: string;
  password: string;
}

function AdminAccessPanel(): JSX.Element {
  const [receipt, setReceipt] = useState<ResetReceipt | null>(null);
  const [owner, setOwner] = useState('jeryu');
  const [repo, setRepo] = useState('jeryu');
  const [login, setLogin] = useState('');
  const [access, setAccess] = useState<'read' | 'write' | 'admin'>('read');
  const users = useQuery({
    queryKey: ['admin', 'users'],
    queryFn: ({ signal }) => apiGet<AdminUser[]>(endpoints.adminUsers(), { signal }),
  });
  const reset = useMutation({
    mutationFn: (target: string) =>
      apiSend<ResetReceipt>(endpoints.adminResetPassword(target), {}),
    onSuccess: setReceipt,
  });
  const grant = useMutation({
    mutationFn: () =>
      apiSend(endpoints.adminRepoGrant(owner, repo, login), { access }),
  });

  return (
    <section className="page__section" aria-labelledby="admin-users">
      <h2 className="page__section-title" id="admin-users">
        Users and repository access
      </h2>
      {users.isPending ? (
        <LoadingState title="Loading users..." variant="message" />
      ) : users.error ? (
        <ErrorState title="Could not load users" error={users.error} />
      ) : (
        <div className="page__card">
          {(users.data ?? []).map((account) => (
            <div className="page__inline-actions" key={account.login}>
              <span className="page__pill">{account.role}</span>
              <strong>{account.login}</strong>
              <ActionButton
                variant="default"
                onClick={() => reset.mutate(account.login)}
              >
                Reset password
              </ActionButton>
            </div>
          ))}
          {receipt ? (
            <p className="page__roadmap-note">
              {receipt.login}: {receipt.password}
            </p>
          ) : null}
        </div>
      )}
      <form
        className="page__card"
        onSubmit={(event) => {
          event.preventDefault();
          grant.mutate();
        }}
      >
        <div className="admin-grant-grid">
          <label>
            Owner
            <input value={owner} onChange={(event) => setOwner(event.currentTarget.value)} />
          </label>
          <label>
            Repo
            <input value={repo} onChange={(event) => setRepo(event.currentTarget.value)} />
          </label>
          <label>
            User
            <input value={login} onChange={(event) => setLogin(event.currentTarget.value)} />
          </label>
          <div className="admin-grant-access">
            <span>Access</span>
            <div className="page__inline-actions" role="radiogroup" aria-label="Access">
              {(['read', 'write', 'admin'] as const).map((value) => (
                <ActionButton
                  key={value}
                  type="button"
                  variant={access === value ? 'primary' : 'default'}
                  role="radio"
                  aria-checked={access === value}
                  onClick={() => setAccess(value)}
                >
                  {value}
                </ActionButton>
              ))}
            </div>
          </div>
        </div>
        <ActionButton type="submit" variant="primary" disabled={grant.isPending}>
          Grant access
        </ActionButton>
        {grant.isSuccess ? <span className="page__pill page__pill--success">Granted</span> : null}
        {grant.error ? <ErrorState title="Could not grant access" error={grant.error} /> : null}
      </form>
    </section>
  );
}

interface ThemeButtonProps {
  value: ThemePreference;
  current: ThemePreference;
  label: string;
  icon: JSX.Element;
  onSelect: (theme: ThemePreference) => void;
}

function ThemeButton({
  value,
  current,
  label,
  icon,
  onSelect,
}: ThemeButtonProps): JSX.Element {
  return (
    <ActionButton
      variant={current === value ? 'primary' : 'default'}
      role="radio"
      aria-checked={current === value}
      icon={icon}
      onClick={() => onSelect(value)}
    >
      {label}
    </ActionButton>
  );
}
