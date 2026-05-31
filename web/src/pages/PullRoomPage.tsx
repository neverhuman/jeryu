// PullRoomPage.tsx — cross-repo Pull Room route; cockpit lands with W-FE-11.

import { NotImplementedRoute } from './NotImplementedRoute';

export function PullRoomPage(): JSX.Element {
  return (
    <NotImplementedRoute
      title="Pull Room"
      workPackage="W-FE-11"
      description="Cross-repo PR cockpit. The route resolves today; the cockpit content lands with W-FE-11."
    />
  );
}
