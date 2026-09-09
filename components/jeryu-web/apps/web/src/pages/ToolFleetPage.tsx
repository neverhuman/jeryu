// ToolFleetPage.tsx — the tool-compounding visibility surface.
//
// Renders the per-tool adoption matrix from `GET /fleet/tool-adoption`: for each
// jankurai tool, which repos have adopted it and which are applicable-but-missing
// (the remaining "should adopt" opportunity). Data is projected from each repo's
// latest recorded score — no extra computation.

import { Boxes, Wrench } from 'lucide-react';

import { EmptyState, ErrorState, LoadingState } from '../components/state';
import { useToolFleet } from '../hooks/useToolFleet';
import type { ToolFleetEntry } from '../api/types';
import './page.css';

function adoptionPillClass(entry: ToolFleetEntry): string {
  const adopted = entry.adopting_repos.length;
  const total = adopted + entry.applicable_missing_repos.length;
  if (total === 0) return 'page__pill';
  if (adopted === total) return 'page__pill page__pill--success';
  if (adopted === 0) return 'page__pill page__pill--danger';
  return 'page__pill page__pill--warning';
}

function ToolRow({ entry }: { entry: ToolFleetEntry }): JSX.Element {
  const adopted = entry.adopting_repos.length;
  const missing = entry.applicable_missing_repos.length;
  const total = adopted + missing;
  return (
    <div className="page__card" data-testid={`tool-row-${entry.tool}`}>
      <div className="page__card-title">
        <Wrench aria-hidden="true" size={16} />
        <span>{entry.tool}</span>
        <span className={adoptionPillClass(entry)}>
          {adopted}/{total} adopted
        </span>
        <span className="page__pill">{entry.category}</span>
      </div>
      <dl className="page__meta-grid">
        <dt>Adopting ({adopted})</dt>
        <dd>{adopted > 0 ? entry.adopting_repos.join(', ') : '—'}</dd>
        <dt>Should adopt ({missing})</dt>
        <dd>
          {missing > 0 ? entry.applicable_missing_repos.join(', ') : '—'}
        </dd>
      </dl>
    </div>
  );
}

export function ToolFleetPage(): JSX.Element {
  const { data, isPending, isError, error } = useToolFleet();

  return (
    <div className="page page--wide" data-testid="tool-fleet-page">
      <header className="page__header">
        <h1 className="page__title">Tool Fleet</h1>
        <p className="page__subtitle">
          Jankurai tool-compounding adoption across the fleet — which repos use
          each shared tool, and which are applicable but haven&apos;t adopted it
          yet. Projected from each repo&apos;s latest recorded audit score.
        </p>
      </header>

      {isPending ? (
        <LoadingState title="Loading tool adoption…" variant="message" />
      ) : isError ? (
        <ErrorState
          title="Could not load tool adoption."
          error={error}
        />
      ) : data.tools.length === 0 ? (
        <EmptyState
          icon={Boxes}
          title="No tool-adoption data yet."
          description="Once repos are scored, their tool adoption appears here."
        />
      ) : (
        <section className="page__section">
          <h2 className="page__section-title">
            {data.tools.length} tools · {data.repos_scored} repos scored
          </h2>
          <div className="page__cards">
            {data.tools.map((entry) => (
              <ToolRow key={entry.tool} entry={entry} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
