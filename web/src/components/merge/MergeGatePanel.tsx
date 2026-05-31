// MergeGatePanel.tsx — Merge Passport verdict surface (W-FE-11).
//
// Shows the §35.2.4 12-gate Passport result. For each blocker we translate
// the canonical `code` (e.g. `passport_blocked_approvals`) into a human
// explanation so reviewers know *why* a gate failed and what to do next.
// Unknown codes fall back to the server-supplied `message`.

import {
  AlertCircle,
  CheckCircle2,
  CircleSlash,
  ShieldAlert,
  ShieldCheck,
  type LucideIcon,
} from 'lucide-react';

import type { MergePassport, MergePassportBlocker } from '../../api/types';

import './merge.css';

/** Per-code human translation. Keys mirror §35.2.4 canonical gate list. */
const GATE_EXPLANATIONS: Record<
  string,
  { title: string; hint: string }
> = {
  passport_blocked_approvals: {
    title: 'Approvals not met',
    hint: 'Required approver count has not been satisfied on the exact head SHA.',
  },
  passport_blocked_codeowners: {
    title: 'CODEOWNERS approval required',
    hint: 'A team listed in CODEOWNERS for the touched paths has not approved.',
  },
  passport_blocked_checks: {
    title: 'Required checks failing',
    hint: 'One or more required status checks is failing, pending, or missing on this head.',
  },
  passport_blocked_threads: {
    title: 'Unresolved review threads',
    hint: 'All conversation threads must be resolved before merging.',
  },
  passport_blocked_branch_protection: {
    title: 'Branch protection violation',
    hint: 'A branch protection rule (linear history, signed commits, etc.) blocks this merge.',
  },
  passport_blocked_policy_sha: {
    title: 'Policy SHA drift',
    hint: 'The policy file changed since this PR was opened. Re-review under the current policy.',
  },
  passport_blocked_passport_sha: {
    title: 'Passport SHA drift',
    hint: 'The Passport verdict was recomputed; refresh and re-attempt.',
  },
  passport_blocked_signed_commits: {
    title: 'Unsigned commits',
    hint: 'Branch protection requires signed commits on every commit in the head range.',
  },
  passport_blocked_linear_history: {
    title: 'Non-linear history',
    hint: 'Branch protection requires a linear history; rebase before merging.',
  },
  passport_blocked_agent_evidence: {
    title: 'Agent evidence required',
    hint: 'Agent-produced patches need attached evidence packets before merge.',
  },
  passport_blocked_license: {
    title: 'License policy violation',
    hint: 'A dependency change introduces a disallowed license.',
  },
  passport_blocked_secret_scan: {
    title: 'Secret scan finding',
    hint: 'A secret-scanning finding is open against this PR.',
  },
};

function explain(blocker: MergePassportBlocker): { title: string; hint: string } {
  const known = GATE_EXPLANATIONS[blocker.code];
  if (known) return known;
  return { title: blocker.code, hint: blocker.message };
}

export interface MergeGatePanelProps {
  passport: MergePassport | null;
  /** When `true`, render a loading skeleton while the passport is computed. */
  isLoading?: boolean;
  className?: string;
}

const StatusIcon: Record<'pass' | 'blocked' | 'pending', LucideIcon> = {
  pass: ShieldCheck,
  blocked: ShieldAlert,
  pending: CircleSlash,
};

export function MergeGatePanel({
  passport,
  isLoading = false,
  className,
}: MergeGatePanelProps): JSX.Element {
  if (isLoading) {
    return (
      <section
        className={`merge-gate ${className ?? ''}`.trim()}
        aria-label="Merge Passport"
      >
        <header className="merge-gate__header merge-gate__header--pending">
          <CircleSlash aria-hidden="true" size={18} />
          <div>
            <h3 className="merge-gate__title">Merge Passport</h3>
            <p className="merge-gate__subtitle">Computing verdict…</p>
          </div>
        </header>
      </section>
    );
  }

  if (!passport) {
    const PendingIcon = StatusIcon.pending;
    return (
      <section
        className={`merge-gate ${className ?? ''}`.trim()}
        aria-label="Merge Passport"
      >
        <header className="merge-gate__header merge-gate__header--pending">
          <PendingIcon aria-hidden="true" size={18} />
          <div>
            <h3 className="merge-gate__title">Merge Passport</h3>
            <p className="merge-gate__subtitle">
              No verdict yet for this head SHA.
            </p>
          </div>
        </header>
      </section>
    );
  }

  const isPass = passport.status === 'pass';
  const Icon = isPass ? StatusIcon.pass : StatusIcon.blocked;
  const tone: 'pass' | 'blocked' = isPass ? 'pass' : 'blocked';

  return (
    <section
      className={`merge-gate ${className ?? ''}`.trim()}
      aria-label="Merge Passport"
      data-status={passport.status}
    >
      <header className={`merge-gate__header merge-gate__header--${tone}`}>
        <Icon aria-hidden="true" size={18} />
        <div>
          <h3 className="merge-gate__title">
            Merge Passport: {isPass ? 'PASS' : 'BLOCKED'}
          </h3>
          <p className="merge-gate__subtitle">
            <span className="merge-gate__sha">{passport.head_sha.slice(0, 7)}</span>
            <span className="merge-gate__meta">
              {' '}· evaluated {new Date(passport.evaluated_at).toLocaleString()}
            </span>
          </p>
        </div>
      </header>

      {isPass ? (
        <p className="merge-gate__pass">
          <CheckCircle2 aria-hidden="true" size={14} /> All 12 gates passed for
          this head.
        </p>
      ) : (
        <ul className="merge-gate__blockers" aria-label="Passport blockers">
          {passport.blockers.map((blocker) => {
            const { title, hint } = explain(blocker);
            return (
              <li
                key={blocker.code + (blocker.details ?? '')}
                className="merge-gate__blocker"
                data-code={blocker.code}
              >
                <AlertCircle
                  aria-hidden="true"
                  size={14}
                  className="merge-gate__blocker-icon"
                />
                <div className="merge-gate__blocker-body">
                  <div className="merge-gate__blocker-title">{title}</div>
                  <div className="merge-gate__blocker-hint">{hint}</div>
                  {blocker.details ? (
                    <div className="merge-gate__blocker-details">
                      {blocker.details}
                    </div>
                  ) : null}
                  <code className="merge-gate__blocker-code">{blocker.code}</code>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
