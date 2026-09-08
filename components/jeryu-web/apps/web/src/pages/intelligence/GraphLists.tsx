// GraphLists.tsx - edge list and cluster chips for the operator graph.

import type { GraphEdge } from '../../api/types';
import type { OperatorGraph } from '../intelligenceGraphModel';

import { compactLabel } from './graphHelpers';
import { SeverityPill, StatePill } from './StateIndicators';

export function EdgeList({ edges }: { edges: GraphEdge[] }): JSX.Element {
  return (
    <section className="intelligence__edge-list">
      <h3>Edges</h3>
      {edges.length === 0 ? (
        <p>No visible edges.</p>
      ) : (
        <ol>
          {edges.slice(0, 12).map((edge) => (
            <li key={`${edge.source}-${edge.target}-${edge.kind}`}>
              <span>{edge.kind}</span>
              <code>{compactLabel(edge.source)}</code>
              <span>→</span>
              <code>{compactLabel(edge.target)}</code>
              <StatePill state={edge.state} />
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

export function ClusterChips({ graph }: { graph: OperatorGraph }): JSX.Element {
  return (
    <section className="intelligence__cluster-chips">
      <h3>Clusters</h3>
      {graph.clusters.length === 0 ? (
        <p>No graph clusters.</p>
      ) : (
        <div>
          {graph.clusters.slice(0, 16).map((cluster) => (
            <span key={cluster.id} className="intelligence__cluster-chip">
              {cluster.label}
              <SeverityPill severity={cluster.severity} />
            </span>
          ))}
        </div>
      )}
    </section>
  );
}
