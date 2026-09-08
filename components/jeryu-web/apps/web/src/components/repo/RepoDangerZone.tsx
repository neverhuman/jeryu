// RepoDangerZone.tsx — destructive repository actions (registry / purge).
//
// Rendered at the bottom of the repository overview. Two rows:
//   1. Remove from registry — keeps the bare storage on disk.
//   2. Purge repository and storage — removes the bare directory too.
//
// Labels come from `available_actions` when the backend advertises
// `repo.delete_registry` / `repo.delete_storage`; the section renders
// regardless (the backend still authorizes every request — a viewer who
// is not allowed to remove the repo gets the error surfaced in the dialog).
//
// The mutation lives in `useDeleteRepository`, which invalidates the repos
// list cache and navigates back to /repos on success.

import { useState } from 'react';

import type { RepositorySummary } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

import { DeleteRepoDialog, type DeleteRepoTier } from './DeleteRepoDialog';
import { useDeleteRepository } from '../../hooks/useDeleteRepository';

import './repo.css';

const REGISTRY_ACTION_ID = 'repo.delete_registry';
const STORAGE_ACTION_ID = 'repo.delete_storage';

export interface RepoDangerZoneProps {
  repo: RepositorySummary;
}

function actionLabel(
  repo: RepositorySummary,
  actionId: string
): string | null {
  const action = repo.available_actions.find(
    (a) => a.action_id === actionId
  );
  return action?.label ?? null;
}

export function RepoDangerZone({ repo }: RepoDangerZoneProps): JSX.Element {
  const fullName = `${repo.id.owner}/${repo.id.name}`;
  const [tier, setTier] = useState<DeleteRepoTier | null>(null);
  const deletion = useDeleteRepository(repo.id.id);

  const registryLabel =
    actionLabel(repo, REGISTRY_ACTION_ID) ?? 'Remove from registry';
  const purgeLabel =
    actionLabel(repo, STORAGE_ACTION_ID) ?? 'Purge repository and storage';

  const openTier = (next: DeleteRepoTier): void => {
    deletion.reset();
    setTier(next);
  };

  return (
    <section
      className="repo-danger-zone"
      aria-label="Danger zone"
      data-testid="repo-danger-zone"
    >
      <h2 className="repo-danger-zone__title">Danger zone</h2>
      <div className="repo-danger-zone__row">
        <div>
          <h3 className="repo-danger-zone__row-title">{registryLabel}</h3>
          <p className="repo-danger-zone__row-detail">
            Removes {fullName} from the registry. The bare storage on disk is
            kept.
          </p>
        </div>
        <ActionButton
          variant="danger"
          actionId={REGISTRY_ACTION_ID}
          onClick={() => openTier('registry')}
        >
          {registryLabel}
        </ActionButton>
      </div>
      <div className="repo-danger-zone__row">
        <div>
          <h3 className="repo-danger-zone__row-title">{purgeLabel}</h3>
          <p className="repo-danger-zone__row-detail">
            Removes {fullName} from the registry and deletes its bare storage
            on disk. Cannot be undone.
          </p>
        </div>
        <ActionButton
          variant="danger"
          actionId={STORAGE_ACTION_ID}
          onClick={() => openTier('purge')}
        >
          {purgeLabel}
        </ActionButton>
      </div>
      <DeleteRepoDialog
        open={tier !== null}
        tier={tier ?? 'registry'}
        fullName={fullName}
        busy={deletion.isPending}
        errorMessage={deletion.error ? deletion.error.message : null}
        confirmLabel={tier === 'purge' ? purgeLabel : registryLabel}
        onCancel={() => setTier(null)}
        onConfirm={() =>
          deletion.mutate({
            confirmFullName: fullName,
            deleteStorage: tier === 'purge',
          })
        }
      />
    </section>
  );
}
