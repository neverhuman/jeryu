// ErrorState.tsx — recoverable failure surface (W-CC-02).
//
// Pass `error` for an ApiError or generic Error; the component pulls `code`
// and `requestId` for debugging when available.

import { AlertTriangle } from 'lucide-react';
import type { ReactNode } from 'react';

import { ApiError } from '../../api/client';

import './state.css';

export interface ErrorStateProps {
  title?: string;
  description?: string;
  error?: unknown;
  action?: ReactNode;
  className?: string;
}

export function ErrorState({
  title = 'Something went wrong.',
  description,
  error,
  action,
  className,
}: ErrorStateProps): JSX.Element {
  let derivedDescription = description;
  let detail: string | null = null;
  if (error instanceof ApiError) {
    derivedDescription = derivedDescription ?? error.message;
    detail = error.requestId
      ? `${error.code} · request ${error.requestId}`
      : error.code;
  } else if (error instanceof Error) {
    derivedDescription = derivedDescription ?? error.message;
  }
  return (
    <div
      className={`state-block ${className ?? ''}`.trim()}
      role="alert"
    >
      <span className="state-block__icon state-block__icon--danger">
        <AlertTriangle aria-hidden="true" size={20} />
      </span>
      <h2 className="state-block__title">{title}</h2>
      {derivedDescription ? (
        <p className="state-block__description">{derivedDescription}</p>
      ) : null}
      {detail ? <p className="state-block__details">{detail}</p> : null}
      {action ? <div className="state-block__action">{action}</div> : null}
    </div>
  );
}
