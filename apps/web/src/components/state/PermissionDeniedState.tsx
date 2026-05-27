// PermissionDeniedState.tsx — 403 surface (W-CC-02).

import { Lock } from 'lucide-react';
import type { ReactNode } from 'react';

import './state.css';

export interface PermissionDeniedStateProps {
  title?: string;
  description?: string;
  /** The permission key the viewer is missing (e.g. `repo.write`). */
  missingPermission?: string;
  action?: ReactNode;
  className?: string;
}

export function PermissionDeniedState({
  title = 'Permission denied',
  description = 'You do not have permission to view this resource.',
  missingPermission,
  action,
  className,
}: PermissionDeniedStateProps): JSX.Element {
  return (
    <div
      className={`state-block ${className ?? ''}`.trim()}
      role="alert"
    >
      <span className="state-block__icon state-block__icon--warning">
        <Lock aria-hidden="true" size={20} />
      </span>
      <h2 className="state-block__title">{title}</h2>
      <p className="state-block__description">{description}</p>
      {missingPermission ? (
        <p className="state-block__details">missing: {missingPermission}</p>
      ) : null}
      {action ? <div className="state-block__action">{action}</div> : null}
    </div>
  );
}
