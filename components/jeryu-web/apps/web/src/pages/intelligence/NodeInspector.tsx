// NodeInspector.tsx - detail panel for the selected operator-graph node.

import type { OperatorGraph } from '../intelligenceGraphModel';

import { StatePill } from './StateIndicators';

export function NodeInspector({ graph }: { graph: OperatorGraph }): JSX.Element {
  if (!graph.selected) {
    return (
      <aside className="intelligence__inspector" data-testid="node-inspector">
        <h3>Selected node</h3>
        <p>No graph nodes available.</p>
      </aside>
    );
  }
  const { node, inbound, outbound, clusters, evidenceCount } = graph.selected;
  return (
    <aside className="intelligence__inspector" data-testid="node-inspector">
      <h3>Selected node</h3>
      <div className="intelligence__inspector-title">
        <strong>{node.label}</strong>
        <StatePill state={node.state} />
      </div>
      <dl>
        <dt>Kind</dt>
        <dd>{node.kind}</dd>
        <dt>Evidence</dt>
        <dd>{evidenceCount}</dd>
        <dt>Edges</dt>
        <dd>{inbound.length} in · {outbound.length} out</dd>
        <dt>Clusters</dt>
        <dd>{clusters.length}</dd>
      </dl>
      <div className="intelligence__metadata">
        {Object.entries(node.metadata).map(([key, value]) =>
          value ? (
            <span key={key}>
              {key}: {value}
            </span>
          ) : null
        )}
      </div>
    </aside>
  );
}
