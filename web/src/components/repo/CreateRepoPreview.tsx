// CreateRepoPreview.tsx — step 2 of the create-repo dialog (W-FE-08).
//
// Renders the normalized side-effect summary the backend returned for the
// dry-run (target, initial files, settings applied, side effects, warnings)
// and the Back / Create actions. The dialog owns the submission flow; this
// component is a read-only review surface plus action wiring.

import type { CreateRepositoryPreview } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

export interface CreateRepoPreviewProps {
  preview: CreateRepositoryPreview | null;
  submitting: boolean;
  onBack: () => void;
  onCreate: () => void;
}

export function CreateRepoPreview({
  preview,
  submitting,
  onBack,
  onCreate,
}: CreateRepoPreviewProps): JSX.Element {
  return (
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
          onClick={onBack}
          type="button"
        >
          Back
        </ActionButton>
        <ActionButton
          variant="primary"
          onClick={onCreate}
          disabled={submitting}
        >
          {submitting ? 'Creating…' : 'Create'}
        </ActionButton>
      </div>
    </div>
  );
}
