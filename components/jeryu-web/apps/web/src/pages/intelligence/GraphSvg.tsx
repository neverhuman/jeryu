// GraphSvg.tsx - SVG rendering of the operator graph and its node marks.

import {
  GRAPH_STATE_ORDER,
  type OperatorGraph,
  type OperatorGraphNode,
} from '../intelligenceGraphModel';

import {
  compactLabel,
  diamondPoints,
  hexPoints,
  nodeRadius,
} from './graphHelpers';

export function GraphSvg({
  graph,
  onSelectNode,
}: {
  graph: OperatorGraph;
  onSelectNode: (id: string) => void;
}): JSX.Element {
  const nodesById = new Map(graph.nodes.map((node) => [node.id, node]));
  if (graph.nodes.length === 0) {
    return (
      <div className="intelligence__graph-empty" data-testid="repo-graph-preview">
        No graph nodes available.
      </div>
    );
  }
  return (
    <div className="intelligence__graph-preview" data-testid="repo-graph-preview">
      <svg viewBox="0 0 980 420" role="img" aria-label="Operator graph">
        <rect
          x="16"
          y="16"
          width="948"
          height="388"
          rx="8"
          className="intelligence__graph-ring"
        />
        {graph.edges.map((edge) => {
          const source = nodesById.get(edge.source);
          const target = nodesById.get(edge.target);
          if (!source || !target) return;
          return (
            <line
              key={`${edge.source}-${edge.target}-${edge.kind}`}
              x1={source.x}
              y1={source.y}
              x2={target.x}
              y2={target.y}
              className={`intelligence__graph-edge is-${edge.state}`}
              strokeWidth={Math.min(5, Math.max(1, edge.weight))}
            />
          );
        })}
        {graph.nodes.map((node) => (
          <GraphNodeMark
            key={node.id}
            node={node}
            selected={graph.selected?.node.id === node.id}
            onSelect={onSelectNode}
          />
        ))}
      </svg>
      <div className="intelligence__legend" aria-label="Graph state legend">
        {GRAPH_STATE_ORDER.map((state) => (
          <span key={state} className={`intelligence__legend-item is-${state}`}>
            {state}
          </span>
        ))}
      </div>
    </div>
  );
}

function GraphNodeMark({
  node,
  selected,
  onSelect,
}: {
  node: OperatorGraphNode;
  selected: boolean;
  onSelect: (id: string) => void;
}): JSX.Element {
  const common = `intelligence__graph-node ${node.colorClass} ${selected ? 'is-selected' : ''}`;
  const label = compactLabel(node.label);
  return (
    <g
      role="button"
      tabIndex={0}
      onClick={() => onSelect(node.id)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') onSelect(node.id);
      }}
      data-testid={`graph-node-${node.id}`}
    >
      <title>{`${node.kind}: ${node.label}`}</title>
      {node.shape === 'circle' ? (
        <circle cx={node.x} cy={node.y} r={nodeRadius(node)} className={common} />
      ) : node.shape === 'diamond' ? (
        <polygon points={diamondPoints(node.x, node.y, nodeRadius(node))} className={common} />
      ) : node.shape === 'hex' ? (
        <polygon points={hexPoints(node.x, node.y, nodeRadius(node) + 3)} className={common} />
      ) : (
        <rect
          x={node.x - nodeRadius(node)}
          y={node.y - nodeRadius(node)}
          width={nodeRadius(node) * 2}
          height={nodeRadius(node) * 2}
          rx="3"
          className={common}
        />
      )}
      <text x={node.x + 12} y={node.y + 4} className="intelligence__graph-label">
        {label}
      </text>
    </g>
  );
}
