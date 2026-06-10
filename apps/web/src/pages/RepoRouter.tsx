// RepoRouter.tsx — dispatches repo sub-pages based on the splat path.
//
// All repo URLs go through `repos/:provider/*`. This component parses the
// splat to extract the owner/name (always the first two segments after
// provider) and the sub-path (agents, code, pulls, settings, blob, issues).
//
// URL examples:
//   /repos/jeryu/jeryu/jankurai          → overview (owner=jeryu, name=jankurai)
//   /repos/jeryu/jeryu/jankurai/agents   → agents page
//   /repos/jeryu/jeryu/jankurai/agents/run-42 → agents page with run-42
//   /repos/jeryu/jeryu/jankurai/code     → code browser
//   /repos/jeryu/jeryu/jankurai/blob/main/src/lib.rs → file viewer

import { useParams } from 'react-router-dom';

import { RepositoryAgentsPage } from './RepositoryAgentsPage';
import { RepositoryCodePage } from './RepositoryCodePage';
import { RepositoryFilePage } from './RepositoryFilePage';
import { RepositoryOverviewPage } from './RepositoryOverviewPage';
import { RepositoryPullRequestsPage } from './RepositoryPullRequestsPage';
import { RepositorySettingsPage } from './RepositorySettingsPage';
import { IssuesPage } from './IssuesPage';
import { PullRequestPage } from './PullRequestPage';

/** Parse the splat into { fullName, subPath, subTail }.
 *
 *  The splat after `:provider/` always starts with `owner/name`.
 *  After that, the next segment (if any) is the sub-page identifier.
 *
 *  Examples:
 *    "jeryu/jankurai"           → { fullName: "jeryu/jankurai", subPath: null, subTail: "" }
 *    "jeryu/jankurai/agents"    → { fullName: "jeryu/jankurai", subPath: "agents", subTail: "" }
 *    "jeryu/jankurai/agents/r1" → { fullName: "jeryu/jankurai", subPath: "agents", subTail: "r1" }
 *    "jeryu/jankurai/blob/main/x" → { fullName: "jeryu/jankurai", subPath: "blob", subTail: "main/x" }
 */
export function parseRepoSplat(splat: string): {
  fullName: string;
  subPath: string | null;
  subTail: string;
} {
  const segments = splat.replace(/\/+$/, '').split('/');
  // First two segments are always owner/name.
  if (segments.length < 2) {
    return { fullName: splat, subPath: null, subTail: '' };
  }
  const fullName = `${segments[0]}/${segments[1]}`;
  if (segments.length === 2) {
    return { fullName, subPath: null, subTail: '' };
  }
  const subPath = segments[2];
  const subTail = segments.slice(3).join('/');
  return { fullName, subPath, subTail };
}

const KNOWN_SUB_PATHS = new Set([
  'agents', 'code', 'blob', 'pulls', 'issues', 'settings',
]);

export function RepoRouter(): JSX.Element {
  const params = useParams();
  const provider = params.provider ?? 'unknown';
  const splat = params['*'] ?? '';
  const { fullName, subPath, subTail } = parseRepoSplat(splat);

  // Dispatch to the correct sub-page based on the sub-path.
  switch (subPath) {
    case 'agents':
      return <RepositoryAgentsPage provider={provider} fullName={fullName} splatTail={subTail} />;
    case 'code':
      return <RepositoryCodePage provider={provider} fullName={fullName} />;
    case 'blob':
      return <RepositoryFilePage provider={provider} fullName={fullName} blobPath={subTail} />;
    case 'pulls': {
      // /pulls or /pulls/:number
      if (subTail) {
        return <PullRequestPage provider={provider} fullName={fullName} prNumber={subTail} />;
      }
      return <RepositoryPullRequestsPage provider={provider} fullName={fullName} />;
    }
    case 'issues':
      return <IssuesPage provider={provider} fullName={fullName} />;
    case 'settings':
      return <RepositorySettingsPage provider={provider} fullName={fullName} section={subTail || undefined} />;
    default:
      // No known sub-path — it's the overview page.
      // The remaining segments (if any) are extra namespace path segments.
      return <RepositoryOverviewPage provider={provider} fullName={fullName} />;
  }
}
