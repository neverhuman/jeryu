// RepoHealthPill.tsx — health indicator (W-FE-08).
//
// The backend serialises `RepositorySummary.health` as a plain string. Three
// canonical values are mapped to colour ramps; anything else falls back to
// the neutral pill so unknown values stay readable in the UI.

import './repo.css';

export type RepoHealth = 'healthy' | 'degraded' | 'failing' | string;

export interface RepoHealthPillProps {
  health: RepoHealth;
  /** Visible label override; defaults to a humanized `health` value. */
  label?: string;
}

function variant(
  health: string
): 'healthy' | 'degraded' | 'failing' | undefined {
  if (health === 'healthy') return 'healthy';
  if (health === 'degraded') return 'degraded';
  if (health === 'failing') return 'failing';
  return;
}

export function RepoHealthPill({
  health,
  label,
}: RepoHealthPillProps): JSX.Element {
  const v = variant(health);
  const className = v
    ? `repo-health-pill repo-health-pill--${v}`
    : 'repo-health-pill';
  const text = label ?? health.replaceAll('_', ' ');
  return (
    <span
      className={className}
      role="status"
      aria-label={`Health: ${text}`}
    >
      <span className="repo-health-pill__dot" aria-hidden="true" />
      {text}
    </span>
  );
}
