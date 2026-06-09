// AdminSettingsPage.tsx — admin preferences surface.
//
// Implements the theme + density preference toggles wired to
// `preferencesStore`. The broader §4.7 settings studio (organization SSO,
// tokens, integrations) is served by the backend admin tier and surfaces
// here once those endpoints exist.

import { Moon, Monitor, Sun, ToggleRight } from 'lucide-react';

import { ActionButton } from '../components/action/ActionButton';
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

  return (
    <div className="page" data-testid="settings-page">
      <header className="page__header">
        <h1 className="page__title">Settings</h1>
        <p className="page__subtitle">
          Admin preferences. The full settings studio (organization SSO,
          tokens, integrations) lands with the backend admin tier.
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
    </div>
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
