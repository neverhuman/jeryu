// LoadingState.tsx — generic loading indicator surface (W-CC-02).
//
// Two visual modes:
//   * `variant="skeleton"` (default) renders the pulse skeleton shapes used
//     on the dashboard and repository overview.
//   * `variant="message"` renders an aria-live message block — preferred for
//     long-running operations where a skeleton would mislead.

import { Loader2 } from 'lucide-react';
import type { ReactNode } from 'react';

import './state.css';

export interface LoadingStateProps {
  title?: string;
  description?: string;
  variant?: 'skeleton' | 'message';
  /** How many skeleton rows to render (default 4). */
  rows?: number;
  className?: string;
  children?: ReactNode;
}

export function LoadingState({
  title = 'Loading…',
  description,
  variant = 'skeleton',
  rows = 4,
  className,
  children,
}: LoadingStateProps): JSX.Element {
  if (variant === 'message') {
    return (
      <div
        className={`state-block ${className ?? ''}`.trim()}
        role="status"
        aria-live="polite"
      >
        <span className="state-block__icon state-block__icon--info">
          <Loader2 aria-hidden="true" size={20} />
        </span>
        <h2 className="state-block__title">{title}</h2>
        {description ? (
          <p className="state-block__description">{description}</p>
        ) : null}
        {children}
      </div>
    );
  }

  return (
    <div
      className={`state-skeleton ${className ?? ''}`.trim()}
      role="status"
      aria-live="polite"
      aria-label={title}
    >
      <span className="sr-only">{title}</span>
      <div className="state-skeleton__row state-skeleton__row--narrow skeleton" />
      {Array.from({ length: rows }).map((_, i) => (
        <div
          key={i}
          className={`state-skeleton__row skeleton ${
            i % 2 === 0 ? 'state-skeleton__row--wide' : ''
          }`}
        />
      ))}
    </div>
  );
}
