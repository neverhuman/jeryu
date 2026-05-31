// CreateRepoForm.tsx — step 1 of the create-repo dialog (W-FE-08).
//
// Renders the editable fields (host/owner/name/description/visibility/
// default branch/topics/initialize_readme) and submits to the parent's
// `onPreview` after validating that owner and name are present. State is
// owned by the dialog via `useCreateRepoDialog`; this component is a
// controlled view over that draft.

import type { RepositoryVisibility } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

import type { DraftRequest } from './useCreateRepoDialog';

export interface CreateRepoFormProps {
  draft: DraftRequest;
  setDraft: React.Dispatch<React.SetStateAction<DraftRequest>>;
  topicsText: string;
  setTopicsText: React.Dispatch<React.SetStateAction<string>>;
  submitting: boolean;
  onPreview: () => void;
  onValidationError: (message: string) => void;
  onCancel: () => void;
}

export function CreateRepoForm({
  draft,
  setDraft,
  topicsText,
  setTopicsText,
  submitting,
  onPreview,
  onValidationError,
  onCancel,
}: CreateRepoFormProps): JSX.Element {
  return (
    <form
      className="create-repo-dialog__form"
      onSubmit={(e) => {
        e.preventDefault();
        if (!draft.owner || !draft.name) {
          onValidationError('Owner and name are required.');
          return;
        }
        onPreview();
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
            <option value="jeryu">jeryu</option>
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
  );
}
