// OperatorGraphConsole.tsx - interactive dependency graph console with filters.

import type { EvidenceState } from '../../api/types';
import {
  GRAPH_STATE_ORDER,
  type GraphFilters,
  type OperatorGraph,
} from '../intelligenceGraphModel';

import { ClusterChips, EdgeList } from './GraphLists';
import { GraphSvg } from './GraphSvg';
import { NodeInspector } from './NodeInspector';
import { toggle } from './graphHelpers';

export function OperatorGraphConsole({
  graph,
  filters,
  onFiltersChange,
  onSelectNode,
}: {
  graph: OperatorGraph;
  filters: GraphFilters;
  onFiltersChange: (filters: GraphFilters) => void;
  onSelectNode: (id: string) => void;
}): JSX.Element {
  return (
    <div className="intelligence__operator" data-testid="operator-graph-console">
      <div className="intelligence__graph-controls">
        <GraphToggles
          title="Kinds"
          options={graph.kindOptions}
          selected={filters.kinds}
          onToggle={(value) =>
            onFiltersChange({ ...filters, kinds: toggle(filters.kinds, value) })
          }
        />
        <GraphToggles
          title="States"
          options={GRAPH_STATE_ORDER}
          selected={filters.states}
          onToggle={(value) =>
            onFiltersChange({
              ...filters,
              states: toggle(filters.states, value as EvidenceState),
            })
          }
        />
        <label className="intelligence__graph-search">
          Search
          <input
            type="search"
            value={filters.query}
            onChange={(event) =>
              onFiltersChange({ ...filters, query: event.target.value })
            }
            aria-label="Search graph"
          />
        </label>
      </div>
      <div className="intelligence__graph-console">
        <GraphSvg graph={graph} onSelectNode={onSelectNode} />
        <NodeInspector graph={graph} />
      </div>
      <div className="intelligence__graph-bottom">
        <EdgeList edges={graph.edges} />
        <ClusterChips graph={graph} />
      </div>
    </div>
  );
}

function GraphToggles({
  title,
  options,
  selected,
  onToggle,
}: {
  title: string;
  options: string[];
  selected: string[];
  onToggle: (value: string) => void;
}): JSX.Element {
  return (
    <fieldset className="intelligence__toggle-set">
      <legend>{title}</legend>
      <div>
        {options.map((option) => (
          <label key={option}>
            <input
              type="checkbox"
              checked={selected.includes(option)}
              onChange={() => onToggle(option)}
            />
            <span>{option}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
