// MirrorStatusBadge.tsx — offsite push-mirror posture pill (W-FE-08 ext).
//
// Consumes `RepositorySummary.mirror` (`RepositoryMirrorStatus`, TS-optional).
// Renders nothing when the field is absent or `configured` is false — most
// repos have no offsite mirror and the meta row should not pay for it.
//
//   * Healthy: cloud-upload icon + relative time of the last successful
//     push ("never pushed" when no success has been recorded yet).
//   * Failing (`last_attempt_ok === false`): danger tokens + alert icon;
//     the title spells out the failure and the age of the last success.

import { CloudAlert, CloudUpload } from 'lucide-react';

import type { RepositoryMirrorStatus } from '../../api/types';

import { relativeTime } from './relativeTime';
import './repo.css';

export interface MirrorStatusBadgeProps {
  mirror?: RepositoryMirrorStatus | null;
}

export function MirrorStatusBadge({
  mirror,
}: MirrorStatusBadgeProps): JSX.Element | null {
  if (!mirror || !mirror.configured) return null;

  const lastSuccess = mirror.last_success_at
    ? relativeTime(mirror.last_success_at)
    : null;
  const failed = mirror.last_attempt_ok === false;

  const text = lastSuccess ?? 'never pushed';
  const detail = failed
    ? `Last mirror push failed · last success ${lastSuccess ?? 'never'}`
    : lastSuccess
      ? `Mirror pushed ${lastSuccess}`
      : 'Mirror configured · never pushed';
  const Icon = failed ? CloudAlert : CloudUpload;

  return (
    <span
      className={
        failed
          ? 'repo-mirror-badge repo-mirror-badge--danger'
          : 'repo-mirror-badge'
      }
      role="status"
      title={detail}
      aria-label={detail}
    >
      <Icon size={12} aria-hidden="true" />
      {text}
    </span>
  );
}
