// BranchProtectionEditor.tsx — list of per-pattern protection rules
// (W-FE-12).
//
// Each rule renders an expandable card with the pattern as the title and a
// form per field. The component is fully controlled — the parent owns the
// rules array and receives a callback when a field changes.

import { Plus, Trash2 } from 'lucide-react';
import type { ChangeEvent } from 'react';

import { ActionButton } from '../action/ActionButton';
import type { BranchProtectionRule } from '../../api/types';

import './settings.css';

export interface BranchProtectionEditorProps {
  rules: BranchProtectionRule[];
  onChange: (rules: BranchProtectionRule[]) => void;
  disabled?: boolean;
  className?: string;
}

const EMPTY_RULE: BranchProtectionRule = {
  pattern: '',
  required_checks: [],
  required_approvals: 0,
  require_signed_commits: false,
  require_conversation_resolution: false,
  allow_force_pushes: false,
  allow_deletions: false,
  bypass_actors: [],
};

export function BranchProtectionEditor({
  rules,
  onChange,
  disabled = false,
  className,
}: BranchProtectionEditorProps): JSX.Element {
  const updateRule = (
    index: number,
    next: Partial<BranchProtectionRule>
  ): void => {
    const updated = rules.map((rule, i) =>
      i === index ? { ...rule, ...next } : rule
    );
    onChange(updated);
  };

  const removeRule = (index: number): void => {
    onChange(rules.filter((_, i) => i !== index));
  };

  const addRule = (): void => {
    onChange([...rules, { ...EMPTY_RULE }]);
  };

  return (
    <div className={`branch-protection ${className ?? ''}`.trim()}>
      {rules.length === 0 ? (
        <p className="branch-protection__empty">
          No branch protection rules configured. Add a rule to require
          approvals or signed commits on a branch pattern.
        </p>
      ) : (
        <ul className="branch-protection__list">
          {rules.map((rule, index) => (
            <li
              key={`${rule.pattern}-${index}`}
              className="branch-protection__rule"
            >
              <div className="branch-protection__row">
                <label
                  className="branch-protection__field branch-protection__field--pattern"
                >
                  <span className="branch-protection__label">
                    Branch pattern
                  </span>
                  <input
                    type="text"
                    value={rule.pattern}
                    onChange={(e: ChangeEvent<HTMLInputElement>) =>
                      updateRule(index, { pattern: e.target.value })
                    }
                    placeholder="main, release/*"
                    disabled={disabled}
                  />
                </label>
                <label className="branch-protection__field">
                  <span className="branch-protection__label">
                    Required approvals
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={20}
                    value={rule.required_approvals}
                    onChange={(e: ChangeEvent<HTMLInputElement>) =>
                      updateRule(index, {
                        required_approvals: Math.max(
                          0,
                          Number.parseInt(e.target.value, 10) || 0
                        ),
                      })
                    }
                    disabled={disabled}
                  />
                </label>
              </div>
              <div className="branch-protection__row">
                <label className="branch-protection__checkbox">
                  <input
                    type="checkbox"
                    checked={rule.require_signed_commits}
                    onChange={(e) =>
                      updateRule(index, {
                        require_signed_commits: e.target.checked,
                      })
                    }
                    disabled={disabled}
                  />
                  Require signed commits
                </label>
                <label className="branch-protection__checkbox">
                  <input
                    type="checkbox"
                    checked={rule.require_conversation_resolution}
                    onChange={(e) =>
                      updateRule(index, {
                        require_conversation_resolution: e.target.checked,
                      })
                    }
                    disabled={disabled}
                  />
                  Require conversation resolution
                </label>
              </div>
              <div className="branch-protection__row">
                <label className="branch-protection__checkbox">
                  <input
                    type="checkbox"
                    checked={rule.allow_force_pushes}
                    onChange={(e) =>
                      updateRule(index, {
                        allow_force_pushes: e.target.checked,
                      })
                    }
                    disabled={disabled}
                  />
                  Allow force pushes
                </label>
                <label className="branch-protection__checkbox">
                  <input
                    type="checkbox"
                    checked={rule.allow_deletions}
                    onChange={(e) =>
                      updateRule(index, {
                        allow_deletions: e.target.checked,
                      })
                    }
                    disabled={disabled}
                  />
                  Allow deletions
                </label>
              </div>
              <div className="branch-protection__row">
                <label className="branch-protection__field branch-protection__field--list">
                  <span className="branch-protection__label">
                    Required checks (comma-separated)
                  </span>
                  <input
                    type="text"
                    value={rule.required_checks.join(', ')}
                    onChange={(e: ChangeEvent<HTMLInputElement>) =>
                      updateRule(index, {
                        required_checks: e.target.value
                          .split(',')
                          .map((s) => s.trim())
                          .filter(Boolean),
                      })
                    }
                    placeholder="ci/build, ci/test"
                    disabled={disabled}
                  />
                </label>
              </div>
              <div className="branch-protection__actions">
                <ActionButton
                  variant="ghost"
                  icon={<Trash2 aria-hidden="true" size={12} />}
                  onClick={() => removeRule(index)}
                  disabled={disabled}
                  aria-label={`Remove rule for ${rule.pattern || 'unnamed pattern'}`}
                >
                  Remove
                </ActionButton>
              </div>
            </li>
          ))}
        </ul>
      )}
      <ActionButton
        variant="default"
        icon={<Plus aria-hidden="true" size={12} />}
        onClick={addRule}
        disabled={disabled}
        className="branch-protection__add"
      >
        Add rule
      </ActionButton>
    </div>
  );
}
