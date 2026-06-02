// SettingsDiffPreview.tsx — preview blast radius + per-field diff (W-FE-12).
//
// Renders the response of `POST /api/v1/repos/{id}/settings/preview`:
//   * Side effects ("affected entities") — which branches / PRs would be
//     touched by the patch.
//   * Per-field diff (before → after).
//   * Warnings (e.g. visibility tightening).
//   * Reversibility flag ("Reversible" / "Not reversible").
//   * Required permission to apply (taken from `side_effects` when the
//     backend tags one; falls back to a generic "settings.write" hint).
//
// The component is render-only — it does not own the apply action; the
// settings page wires a confirm button to the apply mutation.

import {
  AlertTriangle,
  RotateCcw,
  ShieldCheck,
  TriangleAlert,
} from 'lucide-react';

import type {
  SettingsDiffPreview as SettingsDiffPreviewWire,
  SettingsFieldChange,
} from '../../api/types';

import './settings.css';

function renderValue(value: string | null): JSX.Element {
  if (value === null) {
    return <span className="settings-diff__null">∅</span>;
  }
  if (value === '') {
    return <span className="settings-diff__null">(empty)</span>;
  }
  return <code className="settings-diff__value">{value}</code>;
}

export interface SettingsDiffPreviewProps {
  preview: SettingsDiffPreviewWire | null;
  /** When true, render a loading skeleton while the preview is computed. */
  isLoading?: boolean;
  className?: string;
}

export function SettingsDiffPreview({
  preview,
  isLoading = false,
  className,
}: SettingsDiffPreviewProps): JSX.Element {
  if (isLoading) {
    return (
      <div className={`settings-diff ${className ?? ''}`.trim()}>
        <p className="settings-diff__loading">Computing preview…</p>
      </div>
    );
  }

  if (!preview) {
    return (
      <div className={`settings-diff ${className ?? ''}`.trim()}>
        <p className="settings-diff__empty">
          Change a field and press Preview to see what would change.
        </p>
      </div>
    );
  }

  const { diffs, side_effects: sideEffects, warnings, reversible } = preview;

  return (
    <div
      className={`settings-diff ${className ?? ''}`.trim()}
      aria-label="Settings change preview"
    >
      <header className="settings-diff__header">
        <h3 className="settings-diff__title">Preview</h3>
        <span
          className={`settings-diff__reversible ${
            reversible
              ? 'settings-diff__reversible--yes'
              : 'settings-diff__reversible--no'
          }`}
        >
          {reversible ? (
            <>
              <RotateCcw aria-hidden="true" size={12} /> Reversible
            </>
          ) : (
            <>
              <TriangleAlert aria-hidden="true" size={12} /> Not reversible
            </>
          )}
        </span>
      </header>

      <div
        className="settings-diff__permission"
        aria-label="Required permission"
      >
        <ShieldCheck aria-hidden="true" size={12} />
        <span>
          Requires <code>settings.write</code>.
        </span>
      </div>

      {diffs.length === 0 ? (
        <p className="settings-diff__no-changes">
          The patch produced no changes against the current snapshot.
        </p>
      ) : (
        <table className="settings-diff__table" aria-label="Field changes">
          <thead>
            <tr>
              <th scope="col">Field</th>
              <th scope="col">Before</th>
              <th scope="col">After</th>
            </tr>
          </thead>
          <tbody>
            {diffs.map((change: SettingsFieldChange) => (
              <tr key={change.field}>
                <th scope="row" className="settings-diff__field">
                  {change.field}
                </th>
                <td className="settings-diff__before">
                  {renderValue(change.before)}
                </td>
                <td className="settings-diff__after">
                  {renderValue(change.after)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {sideEffects.length > 0 ? (
        <div className="settings-diff__side-effects">
          <h4 className="settings-diff__subheading">Affected entities</h4>
          <ul className="settings-diff__list">
            {sideEffects.map((effect, index) => (
              <li key={`${effect}-${index}`}>{effect}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {warnings.length > 0 ? (
        <div className="settings-diff__warnings" role="alert">
          <h4 className="settings-diff__subheading settings-diff__subheading--warning">
            <AlertTriangle aria-hidden="true" size={12} />
            Warnings
          </h4>
          <ul className="settings-diff__list">
            {warnings.map((warning, index) => (
              <li key={`${warning}-${index}`}>{warning}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
