// mocks.ts — deterministic fixtures (W-T-05 start).
//
// These fixtures are consumed by the MSW handlers (`./server.ts`) and by
// component tests. They mirror the wire contract in
// `contracts/generated/WebBootstrap.ts`.

import type { WebBootstrap } from '../api/types';

export const MOCK_VIEWER_LOGIN = 'codex-tester';

export function makeBootstrapFixture(
  override: Partial<WebBootstrap> = {}
): WebBootstrap {
  const base: WebBootstrap = {
    generated_at: '2026-05-27T00:00:00Z',
    schema_version: '0.1.0-alpha',
    viewer: {
      id: 'usr_codex',
      login: MOCK_VIEWER_LOGIN,
      display_name: 'Codex Tester',
      avatar_url: null,
      global_permissions: ['repo.read', 'web.connect'],
    },
    tui: {},
    recent_repositories: [],
    websocket_url: '/api/v1/ws',
    feature_flags: {
      repo_create: false,
      settings_write: false,
      merge_write: false,
      markdown_html: true,
      agents: false,
      mcp: false,
      workcells: false,
    },
  };
  return { ...base, ...override };
}
