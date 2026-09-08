// useCreateRepoDialog.ts — state controller for the 2-step create-repo dialog
// (W-FE-08).
//
// Owns the draft request, topics text, wizard step, in-flight/error state, and
// the panel ref, plus the preview → create submission flow against
// `/api/v1/repos/preview` (dry_run) and `/api/v1/repos` (with a fresh
// idempotency key). It also resets on close and wires the Escape-to-cancel
// shortcut. The dialog component renders the result; this hook keeps the flow
// out of the view body.

import { useCallback, useEffect, useRef, useState } from 'react';

import { apiSend, ApiError } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import type {
  CreateRepositoryPreview,
  CreateRepositoryRequest,
  RepositorySummary,
} from '../../api/types';

export type DraftRequest = Omit<CreateRepositoryRequest, 'dry_run'>;

export function emptyDraft(host: string, owner: string): DraftRequest {
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

export function randomUuid(): string {
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

export interface UseCreateRepoDialogArgs {
  open: boolean;
  onCancel: () => void;
  onCreated?: (repo: RepositorySummary) => void;
  defaultHost: string;
  defaultOwner: string;
}

export interface UseCreateRepoDialogResult {
  step: 'form' | 'preview';
  setStep: React.Dispatch<React.SetStateAction<'form' | 'preview'>>;
  draft: DraftRequest;
  setDraft: React.Dispatch<React.SetStateAction<DraftRequest>>;
  topicsText: string;
  setTopicsText: React.Dispatch<React.SetStateAction<string>>;
  preview: CreateRepositoryPreview | null;
  submitting: boolean;
  error: string | null;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
  panelRef: React.RefObject<HTMLDivElement | null>;
  handlePreview: () => Promise<void>;
  handleCreate: () => Promise<void>;
}

export function useCreateRepoDialog({
  open,
  onCancel,
  onCreated,
  defaultHost,
  defaultOwner,
}: UseCreateRepoDialogArgs): UseCreateRepoDialogResult {
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
    if (!open) return () => {};
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

  return {
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
  };
}
