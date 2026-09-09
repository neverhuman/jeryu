export interface WorkSummaryProps {
  shown: number;
  total: number;
  blocked: number;
  inReview: number;
}

export function WorkSummary({
  shown,
  total,
  blocked,
  inReview,
}: WorkSummaryProps): JSX.Element {
  return (
    <div className="work-page__summary" aria-label="Work summary">
      <Metric label="shown" value={shown} />
      <Metric label="total" value={total} />
      <Metric label="blocked" value={blocked} />
      <Metric label="review" value={inReview} />
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }): JSX.Element {
  return (
    <div className="work-page__metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
