// AgentPolicyEditor.tsx — autonomous agent policy fields (W-FE-12).
//
// Mirrors `AgentSettings` from contracts/generated. The autonomous toggle
// disables the rest of the form when off, matching the §6 / §35.2.4 policy
// of "evidence required for any agent-produced merge".

import type { ChangeEvent } from 'react';

import type { AgentSettings } from '../../api/types';

import './settings.css';

export interface AgentPolicyEditorProps {
  value: AgentSettings;
  onChange: (next: AgentSettings) => void;
  disabled?: boolean;
  className?: string;
}

export function AgentPolicyEditor({
  value,
  onChange,
  disabled = false,
  className,
}: AgentPolicyEditorProps): JSX.Element {
  const set = (patch: Partial<AgentSettings>): void => {
    onChange({ ...value, ...patch });
  };

  const onListField =
    (field: 'allowed_agents' | 'allowed_tools') =>
    (event: ChangeEvent<HTMLInputElement>): void => {
      const items = event.target.value
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      set({ [field]: items } as Partial<AgentSettings>);
    };

  const onBudget = (event: ChangeEvent<HTMLInputElement>): void => {
    if (event.target.value === '') {
      set({ budget_daily_usd: null });
      return;
    }
    const next = Number.parseFloat(event.target.value);
    set({ budget_daily_usd: Number.isFinite(next) ? next : null });
  };

  return (
    <fieldset
      className={`agent-policy ${className ?? ''}`.trim()}
      disabled={disabled}
    >
      <legend className="sr-only">Agent policy</legend>

      <label className="agent-policy__checkbox agent-policy__checkbox--master">
        <input
          type="checkbox"
          checked={value.autonomous_coding_enabled}
          onChange={(e) =>
            set({ autonomous_coding_enabled: e.target.checked })
          }
        />
        <span>Autonomous coding enabled</span>
      </label>

      <fieldset
        className="agent-policy__group"
        disabled={!value.autonomous_coding_enabled || disabled}
      >
        <legend className="agent-policy__group-title">Concurrency</legend>
        <label className="agent-policy__field">
          <span>Max concurrent sessions</span>
          <input
            type="number"
            min={0}
            max={50}
            value={value.max_concurrent_sessions}
            onChange={(e) =>
              set({
                max_concurrent_sessions: Math.max(
                  0,
                  Number.parseInt(e.target.value, 10) || 0
                ),
              })
            }
          />
        </label>
        <label className="agent-policy__field">
          <span>Daily budget (USD)</span>
          <input
            type="number"
            min={0}
            step={1}
            value={value.budget_daily_usd ?? ''}
            onChange={onBudget}
          />
        </label>
      </fieldset>

      <fieldset
        className="agent-policy__group"
        disabled={!value.autonomous_coding_enabled || disabled}
      >
        <legend className="agent-policy__group-title">Allowed agents</legend>
        <label className="agent-policy__field">
          <span>Comma-separated agent ids</span>
          <input
            type="text"
            value={value.allowed_agents.join(', ')}
            onChange={onListField('allowed_agents')}
          />
        </label>
        <label className="agent-policy__field">
          <span>Allowed tools</span>
          <input
            type="text"
            value={value.allowed_tools.join(', ')}
            onChange={onListField('allowed_tools')}
          />
        </label>
      </fieldset>

      <fieldset
        className="agent-policy__group"
        disabled={!value.autonomous_coding_enabled || disabled}
      >
        <legend className="agent-policy__group-title">Evidence</legend>
        <label className="agent-policy__checkbox">
          <input
            type="checkbox"
            checked={value.evidence_required}
            onChange={(e) => set({ evidence_required: e.target.checked })}
          />
          Require evidence packet for every patch
        </label>
        <label className="agent-policy__checkbox">
          <input
            type="checkbox"
            checked={value.require_human_approval_for_writes}
            onChange={(e) =>
              set({ require_human_approval_for_writes: e.target.checked })
            }
          />
          Require human approval before agent writes
        </label>
      </fieldset>
    </fieldset>
  );
}
