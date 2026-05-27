// SettingsSection.tsx — header + body wrapper for a settings panel
// (W-FE-12).
//
// Provides a consistent visual rhythm: title row, optional description,
// optional inline actions (Preview / Apply buttons), and the children form
// content. Sections own their own forms.

import type { ReactNode } from 'react';

import './settings.css';

export interface SettingsSectionProps {
  title: string;
  description?: string;
  /** Inline action area (e.g. Preview / Apply). */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function SettingsSection({
  title,
  description,
  actions,
  children,
  className,
}: SettingsSectionProps): JSX.Element {
  return (
    <section
      className={`settings-section ${className ?? ''}`.trim()}
      aria-label={title}
    >
      <header className="settings-section__header">
        <div className="settings-section__heading">
          <h2 className="settings-section__title">{title}</h2>
          {description ? (
            <p className="settings-section__description">{description}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="settings-section__actions">{actions}</div>
        ) : null}
      </header>
      <div className="settings-section__body">{children}</div>
    </section>
  );
}
