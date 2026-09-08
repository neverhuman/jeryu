// NotImplementedRoute.tsx — shared chrome for routes that resolve but whose
// feature content ships in a later work package.
//
// The route, breadcrumbs, and selection store are wired so the navigation
// shell behaves correctly; the body explains which work package owns the
// feature so reviewers can see at a glance what is and is not available in
// the current build.

import { Wrench } from 'lucide-react';
import type { ReactNode } from 'react';

import { ActionButton } from '../components/action/ActionButton';
import { EmptyState } from '../components/state';

import './page.css';

export interface NotImplementedRouteProps {
  title: string;
  /** Tracking reference for the work package that delivers this route. */
  workPackage: string;
  description?: string;
  testId?: string;
  /** Optional content to render above the empty state. */
  preface?: ReactNode;
}

export function NotImplementedRoute({
  title,
  workPackage,
  description,
  testId,
  preface,
}: NotImplementedRouteProps): JSX.Element {
  return (
    <div className="page" data-testid={testId}>
      <header className="page__header">
        <h1 className="page__title">{title}</h1>
        {description ? (
          <p className="page__subtitle">{description}</p>
        ) : null}
        <div className="page__inline-actions">
          <span className="page__pill page__pill--warning">
            Planned · {workPackage}
          </span>
        </div>
      </header>
      {preface}
      <EmptyState
        icon={Wrench}
        title={`${title} is not available in this build`}
        description={`The route, breadcrumbs, and selection store are wired; the feature content is delivered by ${workPackage}.`}
        action={
          <ActionButton variant="ghost" disabled>
            Awaiting {workPackage}
          </ActionButton>
        }
      />
    </div>
  );
}
