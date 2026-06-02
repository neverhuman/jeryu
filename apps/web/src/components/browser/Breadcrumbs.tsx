// Breadcrumbs.tsx — host / owner / repo / path crumbs (W-FE-09 + W-FE-10).
//
// Pure structural component. Renders ordered segments and a separator chip
// between them. Each segment may carry a `to` link or be plain text (the
// final crumb is typically inert and announced as `aria-current="page"`).

import { Link } from 'react-router-dom';

import './browser.css';

export interface BreadcrumbSegment {
  label: string;
  to?: string;
  /** Accessible label override; defaults to `label`. */
  ariaLabel?: string;
}

export interface BreadcrumbsProps {
  segments: BreadcrumbSegment[];
  /** Custom separator; defaults to "/". */
  separator?: string;
  /** Aria label for the nav element; defaults to "Breadcrumb". */
  ariaLabel?: string;
}

export function Breadcrumbs({
  segments,
  separator = '/',
  ariaLabel = 'Breadcrumb',
}: BreadcrumbsProps): JSX.Element {
  return (
    <nav className="breadcrumbs" aria-label={ariaLabel}>
      <ol className="breadcrumbs__list">
        {segments.map((seg, idx) => {
          const isLast = idx === segments.length - 1;
          return (
            <li
              key={`${seg.label}-${idx}`}
              className="breadcrumbs__item"
            >
              {seg.to && !isLast ? (
                <Link
                  to={seg.to}
                  className="breadcrumbs__link"
                  aria-label={seg.ariaLabel ?? seg.label}
                >
                  {seg.label}
                </Link>
              ) : (
                <span
                  className={
                    isLast ? 'breadcrumbs__current' : 'breadcrumbs__link'
                  }
                  aria-current={isLast ? 'page' : undefined}
                >
                  {seg.label}
                </span>
              )}
              {!isLast ? (
                <span
                  className="breadcrumbs__separator"
                  aria-hidden="true"
                >
                  {separator}
                </span>
              ) : null}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
