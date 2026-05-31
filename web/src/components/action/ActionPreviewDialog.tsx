// ActionPreviewDialog.tsx — modal shell for action confirmation (W-FE-13).
//
// Renders the dialog frame (focus trap, dismiss affordances, title) into
// which the action preview payload, will-not-do bullet list, and execute
// round-trip are composed by W-FE-13.

import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';

import { ActionButton } from './ActionButton';
import { RiskBadge, type RiskTier } from './RiskBadge';

import './action.css';

export interface ActionPreviewDialogProps {
  open: boolean;
  title: string;
  description?: string;
  risk?: RiskTier;
  onConfirm: () => void;
  onCancel: () => void;
  confirmLabel?: string;
  cancelLabel?: string;
}

export function ActionPreviewDialog({
  open,
  title,
  description,
  risk,
  onConfirm,
  onCancel,
  confirmLabel = 'Run',
  cancelLabel = 'Cancel',
}: ActionPreviewDialogProps): JSX.Element | null {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
    };
    panelRef.current?.focus();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      className="action-preview-dialog__backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        className="action-preview-dialog__panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="action-preview-title"
        tabIndex={-1}
        ref={panelRef}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <h2 id="action-preview-title" className="state-block__title">
              {title}
            </h2>
            {risk ? <RiskBadge tier={risk} /> : null}
          </div>
          <ActionButton
            variant="ghost"
            onClick={onCancel}
            aria-label="Close"
            icon={<X size={14} />}
          />
        </div>
        {description ? (
          <p className="state-block__description">{description}</p>
        ) : null}
        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <ActionButton variant="ghost" onClick={onCancel}>
            {cancelLabel}
          </ActionButton>
          <ActionButton variant="primary" onClick={onConfirm}>
            {confirmLabel}
          </ActionButton>
        </div>
      </div>
    </div>
  );
}
