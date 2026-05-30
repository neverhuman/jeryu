// StubPage.tsx — shared layout for the Phase 1 stub pages.
//
// Each W-FE-08..12 work package replaces its corresponding stub with the
// real implementation. Keeping the chrome consistent makes it obvious to
// reviewers which routes are not yet wired.

import { Wrench } from 'lucide-react';
import type { ReactNode } from 'react';

import { ActionButton } from '../components/action/ActionButton';
import { EmptyState } from '../components/state';

import './page.css';

export interface StubPageProps {
  title: string;
  workPackage: string;
  description?: string;
  /** Optional content to render above the empty state placeholder. */
  preface?: ReactNode;
}

export function StubPage({
  title,
  workPackage,
  description,
  preface,
}: StubPageProps): JSX.Element {
  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">{title}</h1>
        {description ? (
          <p className="page__subtitle">{description}</p>
        ) : null}
        <div className="page__inline-actions">
          <span className="page__pill page__pill--warning">
            Stub · {workPackage}
          </span>
        </div>
      </header>
      {preface}
      <EmptyState
        icon={Wrench}
        title={`${workPackage} not implemented`}
        description={`This route is reserved for ${workPackage}. The route, breadcrumbs, and selection store are wired so downstream work can plug in.`}
        action={
          <ActionButton variant="ghost" disabled>
            Pending implementation
          </ActionButton>
        }
      />
    </div>
  );
}
