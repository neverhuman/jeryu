// AuditPage.tsx — audit timeline route; feature content lands with W-CC-05.

import { NotImplementedRoute } from './NotImplementedRoute';

export function AuditPage(): JSX.Element {
  return (
    <NotImplementedRoute
      title="Audit"
      workPackage="W-CC-05 / W-FE-15"
      description="Audit timeline backed by `web_action_receipts` + `audit_events`."
      testId="audit-page"
    />
  );
}
