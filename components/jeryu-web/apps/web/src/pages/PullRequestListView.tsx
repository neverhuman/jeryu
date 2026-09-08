import {
  AlertTriangle,
  CheckCircle2,
  Clock3,
  FileText,
  GitMerge,
  GitPullRequest,
  type LucideIcon,
} from 'lucide-react';
import { Link } from 'react-router-dom';

import type { PullLane, PullListItem } from './pullRoomModel';

import './PullRoomPage.css';

export function PullRequestCard({
  item,
}: {
  item: PullListItem;
}): JSX.Element {
  const Icon = postureIcon(item.checkPosture);
  return (
    <article
      className={`pull-card is-${item.checkPosture}`}
      data-testid={`pull-card-${item.repo}-${item.number}`}
    >
      <div className="pull-card__top">
        <span className="pull-card__repo">{item.repo}</span>
        <span className="pull-card__number">#{item.number}</span>
      </div>
      <h3 className="pull-card__title">
        <Link to={item.url}>{item.title}</Link>
      </h3>
      <div className="pull-card__refs">
        <code>{item.headRef}</code>
        <span aria-hidden="true">→</span>
        <code>{item.baseRef}</code>
      </div>
      <div className="pull-card__facts">
        <span className="pull-card__pill">{item.draft ? 'draft' : item.state}</span>
        <span className="pull-card__pill">
          <Icon size={12} aria-hidden="true" />
          {item.checkPosture}
        </span>
        <span className="pull-card__pill">
          <GitMerge size={12} aria-hidden="true" />
          {item.mergeable ? 'mergeable' : item.mergeableState}
        </span>
        <span className="pull-card__pill">
          <FileText size={12} aria-hidden="true" />
          {item.changedFileCount} files
        </span>
        <span className="pull-card__pill">evidence {item.evidenceState}</span>
      </div>
      <div className="pull-card__sha">
        <span>head</span>
        <code>{compactSha(item.headSha)}</code>
        <span>base</span>
        <code>{compactSha(item.baseSha)}</code>
      </div>
    </article>
  );
}

export function PullRequestListView({
  lanes,
  emptyMessage,
}: {
  lanes: PullLane[];
  emptyMessage: string;
}): JSX.Element {
  const total = lanes.reduce((sum, lane) => sum + lane.items.length, 0);
  if (total === 0) {
    return <p className="pull-list__empty">{emptyMessage}</p>;
  }
  return (
    <div className="pull-lanes" data-testid="pull-lanes">
      {lanes.map((lane) => (
        <section
          key={lane.id}
          className="pull-lane"
          aria-labelledby={`pull-lane-${lane.id}`}
          data-testid={`pull-lane-${lane.id}`}
        >
          <header className="pull-lane__header">
            <h2 id={`pull-lane-${lane.id}`}>{lane.title}</h2>
            <span className="pull-lane__count">{lane.items.length}</span>
          </header>
          <div className="pull-lane__cards">
            {lane.items.map((item) => (
              <PullRequestCard key={`${item.repo}#${item.number}`} item={item} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function postureIcon(posture: PullListItem['checkPosture']): LucideIcon {
  if (posture === 'passing') return CheckCircle2;
  if (posture === 'queued' || posture === 'running') return Clock3;
  if (posture === 'missing' || posture === 'failing') return AlertTriangle;
  return GitPullRequest;
}

function compactSha(sha: string): string {
  return sha && sha !== 'unknown' ? sha.slice(0, 8) : 'unknown';
}
