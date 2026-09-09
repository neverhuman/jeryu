// toolStatusProductCopy.ts — product copy for the registry tool-status
// vocabulary (the lifecycle words tools-registry.toml uses). Centralizing the
// user-facing strings here keeps lifecycle vocabulary out of component logic:
// components iterate this table and read counts via `countKey`, never
// spelling the status words inline.

import type { ToolRegistrySummary } from '../../api/types';

/** One status chip's copy + the summary field that carries its count. */
export interface ToolStatusCopy {
  /** Registry status value AND user-facing chip label. */
  label: string;
  /** The `ToolRegistrySummary` field holding this status's tool count. */
  countKey: keyof Pick<
    ToolRegistrySummary,
    'published_count' | 'building_count' | 'proposed_count' | 'deprecated_count'
  >;
}

/** Lifecycle order, matching docs/tools-registry.md. */
export const TOOL_STATUS_COPY: readonly ToolStatusCopy[] = [
  { label: 'published', countKey: 'published_count' },
  { label: 'building', countKey: 'building_count' },
  { label: 'proposed', countKey: 'proposed_count' },
  { label: 'deprecated', countKey: 'deprecated_count' },
] as const;

/** The retired-lifecycle label (last entry), for tests and styling hooks. */
export const RETIRED_STATUS_LABEL =
  TOOL_STATUS_COPY[TOOL_STATUS_COPY.length - 1].label;
