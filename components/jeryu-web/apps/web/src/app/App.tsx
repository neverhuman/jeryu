// App.tsx — top-level component (W-FE-02).
//
// Wraps the router with the provider stack. Kept intentionally thin so the
// provider order is auditable in one place.

import { RouterProvider } from 'react-router-dom';

import { AppProviders } from './providers';
import { router } from './router';

export function App(): JSX.Element {
  return (
    <AppProviders>
      <RouterProvider router={router} />
    </AppProviders>
  );
}
