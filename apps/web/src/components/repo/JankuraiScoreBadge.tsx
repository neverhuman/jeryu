// JankuraiScoreBadge.tsx — jankurai audit score pill for repo surfaces.
//
// The backend ships the newest default-branch audit on `RepositorySummary`
// (`jankurai_score` / `jankurai_decision` / `jankurai_scored_at`, all
// TS-optional). Four states:
//   * score >= 85           → good (success tokens)
//   * score <  85           → warn (warning tokens)
//   * no score + a decision → "audit failed" warn pill (the tool ran but
//                              could not score the tree, e.g. `tool-failed`)
//   * no score, no decision → neutral "no score" (no audit ingested yet)
//
// The title/aria-label carry the numeric score plus a relative "scored
// <when>" so the pill stays compact while hover/AT get the full context.

import { Gauge } from 'lucide-react';

import { relativeTime } from './relativeTime';
import './repo.css';

/** Scores at or above this are rendered with success tokens. */
export const JANKURAI_GOOD_THRESHOLD = 85;

export interface JankuraiScoreBadgeProps {
  score?: number | null;
  decision?: string | null;
  scoredAt?: string | null;
}

type Variant = 'good' | 'warn' | 'neutral';

function scoredSuffix(scoredAt: string | null | undefined): string {
  return scoredAt ? ` · scored ${relativeTime(scoredAt)}` : '';
}

function resolve(
  score: number | null | undefined,
  decision: string | null | undefined,
  scoredAt: string | null | undefined
): { variant: Variant; text: string; detail: string } {
  if (score === null || score === undefined) {
    if (decision !== null && decision !== undefined) {
      return {
        variant: 'warn',
        text: 'audit failed',
        detail: `jankurai audit failed (${decision})${scoredSuffix(scoredAt)}`,
      };
    }
    return {
      variant: 'neutral',
      text: 'no score',
      detail: 'No jankurai audit recorded for this repository.',
    };
  }
  const variant: Variant = score >= JANKURAI_GOOD_THRESHOLD ? 'good' : 'warn';
  return {
    variant,
    text: String(score),
    detail: `jankurai score ${score}${scoredSuffix(scoredAt)}`,
  };
}

export function JankuraiScoreBadge({
  score,
  decision,
  scoredAt,
}: JankuraiScoreBadgeProps): JSX.Element {
  const { variant, text, detail } = resolve(score, decision, scoredAt);
  return (
    <span
      className={`repo-score-badge repo-score-badge--${variant}`}
      role="status"
      title={detail}
      aria-label={detail}
    >
      <Gauge size={12} aria-hidden="true" />
      {text}
    </span>
  );
}
