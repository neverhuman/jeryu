// ActionButton.tsx — shared button primitive (variants, icon, disabled).
// W-FE-13 wires the action-preview round-trip onto this primitive.

import type { ButtonHTMLAttributes, ReactNode } from 'react';

import './action.css';

export type ActionButtonVariant = 'default' | 'primary' | 'danger' | 'ghost';

export interface ActionButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Backend action id (e.g. `repo.create`). Reserved for W-FE-13 wiring. */
  actionId?: string;
  /** Optional preview/execute parameters; pass-through to W-FE-13. */
  params?: Record<string, unknown>;
  variant?: ActionButtonVariant;
  icon?: ReactNode;
}

export function ActionButton({
  actionId: _actionId,
  params: _params,
  variant = 'default',
  icon,
  children,
  className,
  type,
  ...rest
}: ActionButtonProps): JSX.Element {
  return (
    <button
      type={type ?? 'button'}
      className={`action-button action-button--${variant} ${
        className ?? ''
      }`.trim()}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}
