// router.tsx — declarative route map (W-FE-02).
//
// Repository URLs use the pattern `/repos/:provider/:owner/:name[/sub-path]`.
// Because `:owner/:name` spans two segments, we cannot use a single `:fullName`
// param for leaf routes (code, agents, pulls, etc.) — React Router's named
// params only match one segment. Instead we use a single catch-all splat
// route `repos/:provider/*` and each repo page component parses the owner,
// name, and sub-path from the splat.
//
// The repoRouteParser utility centralizes this parsing.

import { createBrowserRouter } from 'react-router-dom';

import { AppShell } from '../layout/AppShell';
import { AdminSettingsPage } from '../pages/AdminSettingsPage';
import { AuditPage } from '../pages/AuditPage';
import { DashboardPage } from '../pages/DashboardPage';
import { FleetPage } from '../pages/FleetPage';
import { IntelligencePage } from '../pages/IntelligencePage';
import { IssuesPage } from '../pages/IssuesPage';
import { PullRequestPage } from '../pages/PullRequestPage';
import { PullRoomPage } from '../pages/PullRoomPage';
import { NotFoundPage } from '../pages/NotFoundPage';
import { NotificationsPage } from '../pages/NotificationsPage';
import { RepositoriesPage } from '../pages/RepositoriesPage';
import { RepositoryAgentsPage } from '../pages/RepositoryAgentsPage';
import { RepositoryCodePage } from '../pages/RepositoryCodePage';
import { RepositoryFilePage } from '../pages/RepositoryFilePage';
import { RepositoryPullRequestsPage } from '../pages/RepositoryPullRequestsPage';
import { RepositoryOverviewPage } from '../pages/RepositoryOverviewPage';
import { RepositorySettingsPage } from '../pages/RepositorySettingsPage';
import { SearchResultsPage } from '../pages/SearchResultsPage';
import { RepoRouter } from '../pages/RepoRouter';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'repos', element: <RepositoriesPage /> },
      { path: 'repos/new', element: <RepositoriesPage mode="create" /> },
      // Single catch-all for all repo routes. The RepoRouter component
      // parses the splat to determine the sub-page.
      {
        path: 'repos/:provider/*',
        element: <RepoRouter />,
      },
      { path: 'pull-room', element: <PullRoomPage /> },
      { path: 'intelligence', element: <IntelligencePage /> },
      { path: 'fleet', element: <FleetPage /> },
      { path: 'notifications', element: <NotificationsPage /> },
      { path: 'audit', element: <AuditPage /> },
      { path: 'search', element: <SearchResultsPage /> },
      { path: 'settings', element: <AdminSettingsPage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
