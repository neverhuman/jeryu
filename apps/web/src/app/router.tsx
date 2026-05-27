// router.tsx — declarative route map (W-FE-02).
//
// Adopts the §35.2.1 route map. `*fullName` in the spec is a React Router
// splat: GitLab nested-group paths like `parent/child/repo` are valid
// repository identifiers, so `:fullName` must accept slashes. We model this
// by keeping `:fullName` as a final-segment param and binding the catch-all
// (`*`) on the overview route. Leaf routes (`/code`, `/blob/*`,
// `/merge-requests`, etc.) sit alongside as siblings so they keep their
// own param scopes.
//
// Pages are imported eagerly for Phase 1; W-FE-10 will React.lazy() the
// Monaco-bearing routes so they stay out of the initial bundle.

import { createBrowserRouter } from 'react-router-dom';

import { AppShell } from '../layout/AppShell';
import { AdminSettingsPage } from '../pages/AdminSettingsPage';
import { AuditPage } from '../pages/AuditPage';
import { DashboardPage } from '../pages/DashboardPage';
import { IssuesPage } from '../pages/IssuesPage';
import { MergeRequestPage } from '../pages/MergeRequestPage';
import { MergeRoomPage } from '../pages/MergeRoomPage';
import { NotFoundPage } from '../pages/NotFoundPage';
import { NotificationsPage } from '../pages/NotificationsPage';
import { RepositoriesPage } from '../pages/RepositoriesPage';
import { RepositoryCodePage } from '../pages/RepositoryCodePage';
import { RepositoryFilePage } from '../pages/RepositoryFilePage';
import { RepositoryMergeRequestsPage } from '../pages/RepositoryMergeRequestsPage';
import { RepositoryOverviewPage } from '../pages/RepositoryOverviewPage';
import { RepositorySettingsPage } from '../pages/RepositorySettingsPage';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'repos', element: <RepositoriesPage /> },
      // /repos/new shares the list page with a `mode` flag. W-FE-08 swaps
      // for a real modal anchored to the list route.
      { path: 'repos/new', element: <RepositoriesPage mode="create" /> },
      // Leaf routes — declared before the catch-all so React Router 7's
      // ranking matches them first when a path ends with the suffix.
      {
        path: 'repos/:provider/:fullName/code',
        element: <RepositoryCodePage />,
      },
      {
        path: 'repos/:provider/:fullName/blob/*',
        element: <RepositoryFilePage />,
      },
      {
        path: 'repos/:provider/:fullName/merge-requests',
        element: <RepositoryMergeRequestsPage />,
      },
      {
        path: 'repos/:provider/:fullName/merge-requests/:iid',
        element: <MergeRequestPage />,
      },
      {
        path: 'repos/:provider/:fullName/issues',
        element: <IssuesPage />,
      },
      {
        path: 'repos/:provider/:fullName/settings/:section?',
        element: <RepositorySettingsPage />,
      },
      // Overview catch-all — handles both single-segment and nested
      // GitLab group paths via the `*` splat (`params['*']` carries the
      // tail). For top-level repositories the user lands on
      // `/repos/:provider/:fullName` and `*` is empty.
      {
        path: 'repos/:provider/:fullName/*',
        element: <RepositoryOverviewPage />,
      },
      {
        path: 'repos/:provider/:fullName',
        element: <RepositoryOverviewPage />,
      },
      { path: 'merge-room', element: <MergeRoomPage /> },
      { path: 'notifications', element: <NotificationsPage /> },
      { path: 'audit', element: <AuditPage /> },
      { path: 'settings', element: <AdminSettingsPage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
