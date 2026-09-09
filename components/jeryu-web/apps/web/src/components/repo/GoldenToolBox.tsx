// GoldenToolBox.tsx — gold "tool control plane" tile for the repositories grid.
//
// A prominent card pinned to the top of the default repositories view that
// summarises the reusable-tool registry owned by the special `jeryu-tool`
// repo (the family's tool control plane). It reads
// `GET /api/v1/tools/registry/summary` via `useToolRegistry` and surfaces, at
// a glance: how many tools exist (with a status breakdown), how many repos
// have adopted tools, realized + anticipated LOC saved, and how many
// candidates/tasks are awaiting a build. The top three tools are listed with
// their kind, status, and saved/estimated LOC.
//
// Defensive by design: the box renders `null` while loading, on error, when
// the endpoint is missing (handler shipping in parallel), or when there are no
// tools yet — so it can never break the existing repositories page. Visually
// it is distinguished from `.repo-family-card` by a gold gradient + border
// (see `.repo-golden-box`).

import { Layers, Sparkles, Users, Wrench } from 'lucide-react';
import { Link } from 'react-router-dom';

import { useToolRegistry } from '../../hooks/useToolRegistry';
import { TOOL_STATUS_COPY } from './toolStatusProductCopy';
import type { ToolRegistryEntry, ToolRegistrySummary } from '../../api/types';

import './repo.css';

/** Drill-down target: the /tools control surface (registry rail + the
 *  system-wide duplicate-code dashboard + live scan). */
export const TOOL_CONTROL_PLANE_HREF = '/tools';

const MAX_LISTED_TOOLS = 3;

/** Compact integer formatting (e.g. `12,840`). */
function formatCount(value: number): string {
  return value.toLocaleString('en-US');
}

/**
 * One line in the status-breakdown chip row, e.g. "1 proposed". Zero counts
 * are dropped upstream so the row only carries states that actually exist.
 */
interface StatusChip {
  label: string;
  count: number;
}

function statusBreakdown(summary: ToolRegistrySummary): StatusChip[] {
  // Lifecycle vocabulary lives in the product-copy table; this component
  // never spells the status words inline.
  return TOOL_STATUS_COPY.map((status) => ({
    label: status.label,
    count: summary[status.countKey],
  })).filter((chip) => chip.count > 0);
}

function ToolRow({ tool }: { tool: ToolRegistryEntry }): JSX.Element {
  // Prefer realized savings; fall back to the estimate when nothing has been
  // realized yet so a proposed tool still advertises its upside.
  const loc =
    tool.loc_saved > 0 ? tool.loc_saved : tool.loc_saved_estimate;
  const realized = tool.loc_saved > 0;
  return (
    <li className="repo-golden-box__tool">
      <span className="repo-golden-box__tool-name">{tool.name}</span>
      <span className="repo-golden-box__tool-meta">
        <span className="repo-golden-box__tool-kind">{tool.kind}</span>
        <span
          className={`repo-golden-box__tool-status repo-golden-box__tool-status--${tool.status}`}
        >
          {tool.status}
        </span>
        <span
          className="repo-golden-box__tool-loc"
          title={
            realized
              ? `${formatCount(loc)} lines of code saved`
              : `~${formatCount(loc)} lines of code saved (estimate)`
          }
        >
          {realized ? '' : '~'}
          {formatCount(loc)} LOC
        </span>
      </span>
    </li>
  );
}

export function GoldenToolBox(): JSX.Element | null {
  const { data, isLoading, isError } = useToolRegistry();

  // Degrade to nothing on loading / error / missing-endpoint / empty registry
  // so the box never breaks the page when the backend is not ready. The shape
  // guards (`typeof tool_count`, `Array.isArray`) also reject a stray 200 that
  // is not actually this DTO (e.g. an SPA fallback HTML page parsed as JSON).
  if (
    isLoading ||
    isError ||
    !data ||
    typeof data.tool_count !== 'number' ||
    data.tool_count === 0 ||
    !Array.isArray(data.tools)
  ) {
    return null;
  }

  const statuses = statusBreakdown(data);
  const topTools = data.tools.slice(0, MAX_LISTED_TOOLS);

  return (
    <Link
      to={TOOL_CONTROL_PLANE_HREF}
      className="repo-card repo-golden-box"
      aria-label={`Open the jeryu-tool control plane (${data.tool_count} reusable tools)`}
    >
      <div className="repo-card__head">
        <h3 className="repo-card__title repo-golden-box__title">
          <Wrench size={16} aria-hidden="true" />
          jeryu-tool
        </h3>
        <span className="repo-role-badge repo-role-badge--tool_control_plane">
          Tool control plane
        </span>
      </div>

      <div className="repo-golden-box__stats">
        <div className="repo-golden-box__stat">
          <span className="repo-golden-box__stat-value">
            <Layers size={14} aria-hidden="true" />
            {formatCount(data.tool_count)}
          </span>
          <span className="repo-golden-box__stat-label">
            tool{data.tool_count === 1 ? '' : 's'}
            {statuses.length > 0 ? (
              <span className="repo-golden-box__statuses">
                {statuses.map((chip) => (
                  <span
                    key={chip.label}
                    className="repo-golden-box__status-chip"
                  >
                    {chip.count} {chip.label}
                  </span>
                ))}
              </span>
            ) : null}
          </span>
        </div>

        <div className="repo-golden-box__stat">
          <span className="repo-golden-box__stat-value">
            <Users size={14} aria-hidden="true" />
            {formatCount(data.adopting_repo_count)}
          </span>
          <span className="repo-golden-box__stat-label">repos adopting</span>
        </div>

        <div className="repo-golden-box__stat">
          <span className="repo-golden-box__stat-value">
            <Sparkles size={14} aria-hidden="true" />
            {formatCount(data.realized_loc_saved)}
          </span>
          <span className="repo-golden-box__stat-label">
            LOC saved
            {data.anticipated_loc_saved > 0 ? (
              <span className="repo-golden-box__stat-sub">
                +{formatCount(data.anticipated_loc_saved)} anticipated
              </span>
            ) : null}
          </span>
        </div>

        <div className="repo-golden-box__stat">
          <span className="repo-golden-box__stat-value">
            {formatCount(data.open_task_count)}
          </span>
          <span className="repo-golden-box__stat-label">
            candidate{data.open_task_count === 1 ? '' : 's'} awaiting build
          </span>
        </div>
      </div>

      {topTools.length > 0 ? (
        <ul className="repo-golden-box__tools" aria-label="Top reusable tools">
          {topTools.map((tool) => (
            <ToolRow key={tool.id} tool={tool} />
          ))}
        </ul>
      ) : null}
    </Link>
  );
}
