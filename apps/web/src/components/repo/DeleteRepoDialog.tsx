// DeleteRepoDialog.tsx — two-tier repository removal confirmation.
//
// Tier `registry` (risk: high): removes the registry entry; the bare
// storage on disk is kept and the repo can be re-registered later.
// Tier `purge` (risk: critical): also removes the bare storage on disk —
// cannot be undone — so the confirm button stays disabled until the user
// types the exact `owner/name`.
//
// Purely presentational: the parent owns the mutation and feeds back
// `busy` / `errorMessage` (rendered with role="alert"). Built on the
// shared <ActionPreviewDialog> shell (children/confirmDisabled/
// confirmVariant extension).

import { useEffect, useId, useState } from 'react';

import { ActionPreviewDialog } from '../action/ActionPreviewDialog';

import './repo.css';

export type DeleteRepoTier = 'registry' | 'purge';

export interface DeleteRepoDialogProps {
  open: boolean;
  tier: DeleteRepoTier;
  /** Repository `owner/name`; the typed confirmation must match exactly. */
  fullName: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Confirm button label (the backend action label when available). */
  confirmLabel?: string;
  /** Mutation in flight; disables the confirm button. */
  busy?: boolean;
  /** ApiError (or other) message surfaced inside the dialog. */
  errorMessage?: string | null;
}

export function DeleteRepoDialog({
  open,
  tier,
  fullName,
  onConfirm,
  onCancel,
  confirmLabel,
  busy = false,
  errorMessage = null,
}: DeleteRepoDialogProps): JSX.Element | null {
  const [typed, setTyped] = useState('');
  const inputId = useId();

  // A fresh open (or a tier switch while open) restarts the typed-name
  // confirmation from scratch — stale confirmation must never carry over
  // into the destructive tier.
  useEffect(() => {
    setTyped('');
  }, [open, tier]);

  if (!open) return null;

  const purge = tier === 'purge';
  const nameConfirmed = !purge || typed === fullName;

  return (
    <ActionPreviewDialog
      open={open}
      title={purge ? 'Purge repository and storage' : 'Remove repository'}
      description={
        purge
          ? `Removes ${fullName} from the registry AND deletes its bare storage on disk. This cannot be undone.`
          : `Removes ${fullName} from the registry. The bare storage on disk is kept, so the repository can be registered again later.`
      }
      risk={purge ? 'critical' : 'high'}
      onConfirm={onConfirm}
      onCancel={onCancel}
      confirmLabel={confirmLabel ?? (purge ? 'Purge everything' : 'Remove')}
      confirmVariant="danger"
      confirmDisabled={busy || !nameConfirmed}
    >
      {errorMessage ? (
        <div className="delete-repo-dialog__error" role="alert">
          {errorMessage}
        </div>
      ) : null}
      {purge ? (
        <div className="delete-repo-dialog__confirm">
          <label htmlFor={inputId}>
            Type <strong>{fullName}</strong> to confirm
          </label>
          <input
            id={inputId}
            type="text"
            className="delete-repo-dialog__confirm-input"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            placeholder={fullName}
          />
        </div>
      ) : null}
    </ActionPreviewDialog>
  );
}
