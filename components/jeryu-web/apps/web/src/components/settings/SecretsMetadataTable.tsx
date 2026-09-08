// SecretsMetadataTable.tsx — list of secret names / scopes / age (W-FE-12).
//
// IMPORTANT — Per §35.2.4 / FINAL §7.4 the SPA never sees secret values.
// This component only renders metadata (name, scope, last rotated, fingerprint).

import { Eye, KeyRound } from 'lucide-react';

import './settings.css';

export interface SecretMetadata {
  name: string;
  scope: string;
  /** RFC3339 timestamp; rendered as "x ago" by the locale-sensitive formatter. */
  rotated_at: string | null;
  fingerprint: string | null;
}

export interface SecretsMetadataTableProps {
  secrets: SecretMetadata[];
  className?: string;
}

function ageLabel(rotatedAt: string | null): string {
  if (!rotatedAt) return 'never rotated';
  const then = new Date(rotatedAt).getTime();
  const now = Date.now();
  const ageMs = now - then;
  if (!Number.isFinite(ageMs) || ageMs < 0) return 'recently';
  const days = Math.floor(ageMs / (1000 * 60 * 60 * 24));
  if (days === 0) return 'today';
  if (days === 1) return '1 day ago';
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} months ago`;
  const years = Math.floor(months / 12);
  return `${years} year${years === 1 ? '' : 's'} ago`;
}

export function SecretsMetadataTable({
  secrets,
  className,
}: SecretsMetadataTableProps): JSX.Element {
  if (secrets.length === 0) {
    return (
      <div className={`secrets-table ${className ?? ''}`.trim()}>
        <p className="secrets-table__empty">
          No secrets configured for this repository.
        </p>
        <p className="secrets-table__hint">
          <Eye aria-hidden="true" size={12} /> Secret values are write-only —
          they are never read back through the web API.
        </p>
      </div>
    );
  }

  return (
    <div
      className={`secrets-table ${className ?? ''}`.trim()}
      aria-label="Secrets"
    >
      <table className="secrets-table__table">
        <thead>
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Scope</th>
            <th scope="col">Last rotated</th>
            <th scope="col">Fingerprint</th>
          </tr>
        </thead>
        <tbody>
          {secrets.map((secret) => (
            <tr key={`${secret.name}-${secret.scope}`}>
              <th scope="row" className="secrets-table__name">
                <KeyRound aria-hidden="true" size={12} />
                {secret.name}
              </th>
              <td>
                <code>{secret.scope}</code>
              </td>
              <td>{ageLabel(secret.rotated_at)}</td>
              <td>
                {secret.fingerprint ? (
                  <code className="secrets-table__fingerprint">
                    {secret.fingerprint}
                  </code>
                ) : (
                  <span className="secrets-table__none">—</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="secrets-table__hint">
        <Eye aria-hidden="true" size={12} /> Secret values are write-only —
        they are never read back through the web API.
      </p>
    </div>
  );
}
