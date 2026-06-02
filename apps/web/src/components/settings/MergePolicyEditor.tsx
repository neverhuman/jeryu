// MergePolicyEditor.tsx — controls for the §6.7 merge policy (W-FE-12).
//
// Fully controlled — the parent owns the `MergeSettings` value and gets a
// new copy per edit. The required-approvals counter is bound to the
// `required_approvals` field; the Merge Passport toggle binds to
// `require_jeryu_merge_passport`.

import type { ChangeEvent } from 'react';

import type { MergeSettings } from '../../api/types';

import './settings.css';

export interface MergePolicyEditorProps {
  value: MergeSettings;
  onChange: (next: MergeSettings) => void;
  disabled?: boolean;
  className?: string;
}

export function MergePolicyEditor({
  value,
  onChange,
  disabled = false,
  className,
}: MergePolicyEditorProps): JSX.Element {
  const set = (patch: Partial<MergeSettings>): void => {
    onChange({ ...value, ...patch });
  };

  const onNumber = (
    field: 'required_approvals'
  ) => (event: ChangeEvent<HTMLInputElement>): void => {
    const next = Math.max(0, Math.min(20, Number.parseInt(event.target.value, 10) || 0));
    set({ [field]: next } as Partial<MergeSettings>);
  };

  return (
    <fieldset
      className={`merge-policy ${className ?? ''}`.trim()}
      disabled={disabled}
    >
      <legend className="sr-only">Merge policy</legend>

      <div className="merge-policy__group">
        <h4 className="merge-policy__group-title">Allowed merge methods</h4>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.allow_merge_commit}
            onChange={(e) => set({ allow_merge_commit: e.target.checked })}
          />
          Allow merge commit
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.allow_squash_merge}
            onChange={(e) => set({ allow_squash_merge: e.target.checked })}
          />
          Allow squash merge
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.allow_rebase_merge}
            onChange={(e) => set({ allow_rebase_merge: e.target.checked })}
          />
          Allow rebase merge
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.allow_auto_merge}
            onChange={(e) => set({ allow_auto_merge: e.target.checked })}
          />
          Allow auto-merge once gates pass
        </label>
      </div>

      <div className="merge-policy__group">
        <h4 className="merge-policy__group-title">Approvals</h4>
        <label className="merge-policy__field">
          <span>Required approvals</span>
          <input
            type="number"
            min={0}
            max={20}
            value={value.required_approvals}
            onChange={onNumber('required_approvals')}
          />
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.dismiss_stale_approvals}
            onChange={(e) => set({ dismiss_stale_approvals: e.target.checked })}
          />
          Dismiss stale approvals when new commits land
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.require_codeowners}
            onChange={(e) => set({ require_codeowners: e.target.checked })}
          />
          Require CODEOWNERS approval
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.require_exact_sha_approval}
            onChange={(e) =>
              set({ require_exact_sha_approval: e.target.checked })
            }
          />
          Require exact SHA approval (no auto-carry on rebase)
        </label>
      </div>

      <div className="merge-policy__group">
        <h4 className="merge-policy__group-title">Branch hygiene</h4>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.delete_branch_on_merge}
            onChange={(e) => set({ delete_branch_on_merge: e.target.checked })}
          />
          Branch deletion on merge
        </label>
        <label className="merge-policy__checkbox">
          <input
            type="checkbox"
            checked={value.require_linear_history}
            onChange={(e) => set({ require_linear_history: e.target.checked })}
          />
          Require linear history
        </label>
      </div>

      <div className="merge-policy__group merge-policy__group--passport">
        <h4 className="merge-policy__group-title">Merge Passport</h4>
        <label className="merge-policy__checkbox merge-policy__checkbox--bold">
          <input
            type="checkbox"
            checked={value.require_jeryu_merge_passport}
            onChange={(e) =>
              set({ require_jeryu_merge_passport: e.target.checked })
            }
          />
          Require JeRyu Merge Passport
        </label>
        <p className="merge-policy__hint">
          When enabled, every merge runs the §35.2.4 12-gate evaluation.
          Disable only for transitional repos.
        </p>
      </div>
    </fieldset>
  );
}
