import type { RepositorySummary } from '../../api/types';

import './repo.css';

export function RepoRoleBadge({
  role,
}: {
  role: RepositorySummary['repo_role'];
}): JSX.Element | null {
  if (!role) return null;
  const label = role === 'public_portal' ? 'Public portal' : 'Split member';
  return (
    <span className={`repo-role-badge repo-role-badge--${role}`}>
      {label}
    </span>
  );
}
