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

import { X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { apiSend, ApiError } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import type {
  CreateRepositoryPreview,
  CreateRepositoryRequest,
  RepositorySummary,
  RepositoryVisibility,
} from '../../api/types';
import { ActionButton } from '../action/ActionButton';

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

type DraftRequest = Omit<CreateRepositoryRequest, 'dry_run'>;

function emptyDraft(host: string, owner: string): DraftRequest {
  return {
    host,
    owner,
    name: '',
    description: null,
    visibility: 'private',
    initialize_readme: true,
    gitignore_template: null,
    license_template: null,
    default_branch: 'main',
    topics: [],
    family: null,
    template: null,
  };
}

function randomUuid(): string {
  if (
    typeof crypto !== 'undefined' &&
    typeof crypto.randomUUID === 'function'
  ) {
    return crypto.randomUUID();
  }
  // RFC4122-ish fallback for environments without crypto.randomUUID
  // (Vitest jsdom + older Node). The dialog's idempotency requirement is
  // "unique per request", not cryptographic, so this is sufficient.
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16);
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

export function CreateRepoDialog({
  open,
  onCancel,
  onCreated,
  defaultHost = 'gitlab',
  defaultOwner = '',
}: CreateRepoDialogProps): JSX.Element | null {
  const [step, setStep] = useState<'form' | 'preview'>('form');
  const [draft, setDraft] = useState<DraftRequest>(() =>
    emptyDraft(defaultHost, defaultOwner)
  );
  const [topicsText, setTopicsText] = useState('');
  const [preview, setPreview] = useState<CreateRepositoryPreview | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    panelRef.current?.focus();
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onCancel]);

  useEffect(() => {
    if (!open) {
      // Reset state when the dialog closes so a re-open starts fresh.
      setStep('form');
      setPreview(null);
      setSubmitting(false);
      setError(null);
      setDraft(emptyDraft(defaultHost, defaultOwner));
      setTopicsText('');
    }
  }, [open, defaultHost, defaultOwner]);

  const buildRequest = useCallback(
    (dryRun: boolean): CreateRepositoryRequest => ({
      ...draft,
      topics: topicsText
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean),
      dry_run: dryRun,
    }),
    [draft, topicsText]
  );

  const handlePreview = useCallback(async () => {
    setSubmitting(true);
    setError(null);
    try {
      const body = buildRequest(true);
      const result = await apiSend<CreateRepositoryPreview>(
        `${endpoints.repos()}/preview`,
        body
      );
      setPreview(result);
      setStep('preview');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Preview failed.');
    } finally {
      setSubmitting(false);
    }
  }, [buildRequest]);

  const handleCreate = useCallback(async () => {
    setSubmitting(true);
    setError(null);
    try {
      const body = buildRequest(false);
      const result = await apiSend<RepositorySummary>(
        endpoints.repos(),
        body,
        { idempotencyKey: randomUuid() }
      );
      onCreated?.(result);
      onCancel();
    } catch (cause) {
      if (cause instanceof ApiError) {
        setError(`${cause.code}: ${cause.message}`);
      } else {
        setError(
          cause instanceof Error ? cause.message : 'Repository creation failed.'
        );
      }
    } finally {
      setSubmitting(false);
    }
  }, [buildRequest, onCreated, onCancel]);

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
          <form
            className="create-repo-dialog__form"
            onSubmit={(e) => {
              e.preventDefault();
              if (!draft.owner || !draft.name) {
                setError('Owner and name are required.');
                return;
              }
              void handlePreview();
            }}
          >
            <div className="create-repo-dialog__row">
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-host"
                >
                  Host
                </label>
                <select
                  id="create-repo-host"
                  className="create-repo-dialog__select"
                  value={draft.host}
                  onChange={(e) =>
                    setDraft({ ...draft, host: e.target.value })
                  }
                >
                  <option value="gitlab">gitlab</option>
                  <option value="github">github</option>
                  <option value="local">local</option>
                </select>
              </div>
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-visibility"
                >
                  Visibility
                </label>
                <select
                  id="create-repo-visibility"
                  className="create-repo-dialog__select"
                  value={draft.visibility}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      visibility: e.target.value as RepositoryVisibility,
                    })
                  }
                >
                  <option value="private">private</option>
                  <option value="internal">internal</option>
                  <option value="public">public</option>
                </select>
              </div>
            </div>

            <div className="create-repo-dialog__row">
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-owner"
                >
                  Owner
                </label>
                <input
                  id="create-repo-owner"
                  className="create-repo-dialog__input"
                  value={draft.owner}
                  onChange={(e) =>
                    setDraft({ ...draft, owner: e.target.value })
                  }
                  required
                  autoComplete="off"
                />
              </div>
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-name"
                >
                  Name
                </label>
                <input
                  id="create-repo-name"
                  className="create-repo-dialog__input"
                  value={draft.name}
                  onChange={(e) =>
                    setDraft({ ...draft, name: e.target.value })
                  }
                  required
                  autoComplete="off"
                />
              </div>
            </div>

            <div className="create-repo-dialog__field">
              <label
                className="create-repo-dialog__label"
                htmlFor="create-repo-description"
              >
                Description
              </label>
              <textarea
                id="create-repo-description"
                className="create-repo-dialog__textarea"
                value={draft.description ?? ''}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    description: e.target.value === '' ? null : e.target.value,
                  })
                }
              />
            </div>

            <div className="create-repo-dialog__row">
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-default-branch"
                >
                  Default branch
                </label>
                <input
                  id="create-repo-default-branch"
                  className="create-repo-dialog__input"
                  value={draft.default_branch ?? ''}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      default_branch:
                        e.target.value === '' ? null : e.target.value,
                    })
                  }
                  autoComplete="off"
                />
              </div>
              <div className="create-repo-dialog__field">
                <label
                  className="create-repo-dialog__label"
                  htmlFor="create-repo-topics"
                >
                  Topics (comma-separated)
                </label>
                <input
                  id="create-repo-topics"
                  className="create-repo-dialog__input"
                  value={topicsText}
                  onChange={(e) => setTopicsText(e.target.value)}
                  placeholder="rust, async, jeryu"
                  autoComplete="off"
                />
              </div>
            </div>

            <label className="create-repo-dialog__checkbox-row">
              <input
                type="checkbox"
                checked={draft.initialize_readme}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    initialize_readme: e.target.checked,
                  })
                }
              />
              Initialize with README
            </label>

            <div className="create-repo-dialog__actions">
              <ActionButton variant="ghost" onClick={onCancel} type="button">
                Cancel
              </ActionButton>
              <ActionButton
                variant="primary"
                type="submit"
                disabled={submitting}
              >
                {submitting ? 'Previewing…' : 'Preview'}
              </ActionButton>
            </div>
          </form>
        ) : (
          <div className="create-repo-dialog__form">
            {preview ? (
              <div className="create-repo-dialog__preview">
                <p className="create-repo-dialog__preview-section-title">
                  Target
                </p>
                <p>
                  {preview.target_owner}/<strong>{preview.normalized_name}</strong>{' '}
                  · {preview.visibility}
                </p>
                {preview.initial_files.length > 0 ? (
                  <>
                    <p className="create-repo-dialog__preview-section-title">
                      Initial files
                    </p>
                    <ul className="create-repo-dialog__preview-list">
                      {preview.initial_files.map((f) => (
                        <li key={f}>{f}</li>
                      ))}
                    </ul>
                  </>
                ) : null}
                {preview.settings_to_apply.length > 0 ? (
                  <>
                    <p className="create-repo-dialog__preview-section-title">
                      Settings applied
                    </p>
                    <ul className="create-repo-dialog__preview-list">
                      {preview.settings_to_apply.map((s) => (
                        <li key={s}>{s}</li>
                      ))}
                    </ul>
                  </>
                ) : null}
                {preview.side_effects.length > 0 ? (
                  <>
                    <p className="create-repo-dialog__preview-section-title">
                      Side effects
                    </p>
                    <ul className="create-repo-dialog__preview-list">
                      {preview.side_effects.map((s) => (
                        <li key={s}>{s}</li>
                      ))}
                    </ul>
                  </>
                ) : null}
                {preview.warnings.length > 0 ? (
                  <>
                    <p className="create-repo-dialog__preview-section-title create-repo-dialog__preview-warning">
                      Warnings
                    </p>
                    <ul className="create-repo-dialog__preview-list">
                      {preview.warnings.map((w) => (
                        <li
                          key={w}
                          className="create-repo-dialog__preview-warning"
                        >
                          {w}
                        </li>
                      ))}
                    </ul>
                  </>
                ) : null}
              </div>
            ) : null}
            <div className="create-repo-dialog__actions">
              <ActionButton
                variant="ghost"
                onClick={() => setStep('form')}
                type="button"
              >
                Back
              </ActionButton>
              <ActionButton
                variant="primary"
                onClick={() => void handleCreate()}
                disabled={submitting}
              >
                {submitting ? 'Creating…' : 'Create'}
              </ActionButton>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
