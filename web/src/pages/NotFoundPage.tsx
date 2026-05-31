// NotFoundPage.tsx — 404 (W-FE-15).

import { useNavigate, useLocation } from 'react-router-dom';
import { MapPinOff } from 'lucide-react';

import { ActionButton } from '../components/action/ActionButton';
import { EmptyState } from '../components/state';

import './page.css';

export function NotFoundPage(): JSX.Element {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <div className="page">
      <EmptyState
        icon={MapPinOff}
        title="Page not found"
        description={`We couldn't find ${location.pathname}. The link may be outdated, or the resource may have been moved.`}
        action={
          <ActionButton variant="primary" onClick={() => navigate('/')}>
            Back to dashboard
          </ActionButton>
        }
      />
    </div>
  );
}
