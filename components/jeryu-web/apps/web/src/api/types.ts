// types.ts — barrel entry for the SPA's wire types (W-FE-03).
//
// The declarations live in per-domain modules under `./types/`; this file is a
// thin composition shell so app code keeps importing from `@/api/types` (the
// logical boundary) with zero deep-path imports. Every module below is
// type-only, so we re-export with `export type *` to stay isolatedModules-safe.
//
// When a new DTO is needed, add it to the relevant `./types/*` module (or, for
// generated DTOs, add the re-export to `./types/generated`) — do not edit the
// generated files.

export type * from './types/generated';
export type * from './types/pullRequests';
export type * from './types/controlPlane';
export type * from './types/toolBuild';

// Live agent terminal types (defined in ./agentTerminal).
export type { AgentTtyFrame, AgentControl, AgentControlClientMessage, RepoAgentSummary, RepoAgentRunsResponse, CreateSessionResponse } from './agentTerminal';
