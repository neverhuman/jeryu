// CreateRepoDialog.tsx — 2-step preview → execute repo create (W-FE-08).
//
// Workflow (§35.1.3):
//   Step 1 (form): user fills in host/owner/name/description/visibility/
//     initialize_readme/default_branch/topics. On "Preview" we POST to
//     `/api/v1/repos/preview` with `dry_run: true` — the backend computes the
//     normalized side effects and returns a `CreateRepositoryPreview` we
//     mirror back to the user.
//   Step 2 (preview): user reviews the planned side effects and clicks
//     "Create". We send the same payload to `/api/v1/repos` with
//     `dry_run: false` and a fresh `Idempotency-Key` (UUIDv4) so retries are
//     safe.
//
// On success the dialog calls `onCreated` with the returned summary so the
// list can refetch.
//
// The form and preview steps live in sibling components (`CreateRepoForm`,
// `CreateRepoPreview`) and the wizard state/flow in `useCreateRepoDialog`;
// this module wires them into the modal shell.

import { X } from 'lucide-react';

import type { RepositorySummary } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

import { CreateRepoForm } from './CreateRepoForm';
import { CreateRepoPreview } from './CreateRepoPreview';
import { useCreateRepoDialog } from './useCreateRepoDialog';

import './repo.css';

export interface CreateRepoDialogProps {
  open: boolean;
  onCancel: () => void;
  onCreated?: (repo: RepositorySummary) => void;
  /** Optional default host pre-selected in step 1. */
  defaultHost?: string;
  /** Optional default owner pre-filled in step 1. */
  defaultOwner?: string;
}

export function CreateRepoDialog({
  open,
  onCancel,
  onCreated,
  defaultHost = 'jeryu',
  defaultOwner = '',
}: CreateRepoDialogProps): JSX.Element | null {
  const {
    step,
    setStep,
    draft,
    setDraft,
    topicsText,
    setTopicsText,
    preview,
    submitting,
    error,
    setError,
    panelRef,
    handlePreview,
    handleCreate,
  } = useCreateRepoDialog({
    open,
    onCancel,
    onCreated,
    defaultHost,
    defaultOwner,
  });

  if (!open) return null;

  return (
    <div
      className="action-preview-dialog__backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        ref={panelRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-repo-dialog-title"
        className="create-repo-dialog"
      >
        <header className="create-repo-dialog__head">
          <div>
            <h2
              id="create-repo-dialog-title"
              className="create-repo-dialog__title"
            >
              Create repository
            </h2>
            <div className="create-repo-dialog__steps" aria-hidden="true">
              <span
                className={
                  step === 'form'
                    ? 'create-repo-dialog__step--current'
                    : undefined
                }
              >
                1. Form
              </span>
              <span>·</span>
              <span
                className={
                  step === 'preview'
                    ? 'create-repo-dialog__step--current'
                    : undefined
                }
              >
                2. Preview
              </span>
            </div>
          </div>
          <ActionButton
            variant="ghost"
            onClick={onCancel}
            aria-label="Close"
            icon={<X size={14} />}
          />
        </header>

        {error ? (
          <div className="create-repo-dialog__error" role="alert">
            {error}
          </div>
        ) : null}

        {step === 'form' ? (
          <CreateRepoForm
            draft={draft}
            setDraft={setDraft}
            topicsText={topicsText}
            setTopicsText={setTopicsText}
            submitting={submitting}
            onPreview={() => void handlePreview()}
            onValidationError={setError}
            onCancel={onCancel}
          />
        ) : (
          <CreateRepoPreview
            preview={preview}
            submitting={submitting}
            onBack={() => setStep('form')}
            onCreate={() => void handleCreate()}
          />
        )}
      </div>
    </div>
  );
}
