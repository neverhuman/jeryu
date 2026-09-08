// fleet/ComponentCell.tsx — one cell of the scm/database/sandbox/cache/vault
// system-health strip on the /fleet dashboard. Extracted verbatim from
// FleetPage.tsx.

import type { FleetComponent } from '../fleetModel';

export function ComponentCell({
  component,
}: {
  component: FleetComponent;
}): JSX.Element {
  return (
    <div
      className="fleet__health-cell"
      data-testid={`fleet-health-${component.name}`}
    >
      <span
        className={`fleet__dot fleet__dot--${component.status}`}
        aria-hidden="true"
      />
      <div className="fleet__health-meta">
        <span className="fleet__health-name">{component.name}</span>
        <span className="fleet__health-detail">
          {component.status}
          {component.latencyMs !== null ? ` · ${component.latencyMs}ms` : ''}
          {component.detail ? ` · ${component.detail}` : ''}
        </span>
      </div>
    </div>
  );
}
