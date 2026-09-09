import type { FormEvent } from 'react';

import type { WorkItemKind, WorkPriority, WorkStatus } from '../../api/types';
import {
  WORK_KINDS,
  WORK_KIND_LABELS,
  WORK_PRIORITIES,
  WORK_PRIORITY_LABELS,
  WORK_STATUSES,
  WORK_STATUS_LABELS,
  type EditState,
} from '../workModel';

export interface WorkDetailEditFormProps {
  current: EditState;
  patchDraft: (change: Partial<EditState>) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  pending: boolean;
  errorMessage: string | null;
}

export function WorkDetailEditForm({
  current,
  patchDraft,
  onSubmit,
  pending,
  errorMessage,
}: WorkDetailEditFormProps): JSX.Element {
  return (
    <form className="work-detail__panel work-detail__edit" onSubmit={onSubmit}>
      <label className="work-detail__wide">
        Title
        <input
          value={current.title}
          onChange={(event) => patchDraft({ title: event.target.value })}
          required
        />
      </label>
      <label>
        Status
        <select
          value={current.status}
          onChange={(event) =>
            patchDraft({ status: event.target.value as WorkStatus })
          }
        >
          {WORK_STATUSES.map((status) => (
            <option key={status} value={status}>
              {WORK_STATUS_LABELS[status]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Kind
        <select
          value={current.kind}
          onChange={(event) =>
            patchDraft({ kind: event.target.value as WorkItemKind })
          }
        >
          {WORK_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {WORK_KIND_LABELS[kind]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Priority
        <select
          value={current.priority}
          onChange={(event) =>
            patchDraft({ priority: event.target.value as WorkPriority })
          }
        >
          {WORK_PRIORITIES.map((priority) => (
            <option key={priority} value={priority}>
              {WORK_PRIORITY_LABELS[priority]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Labels
        <input
          value={current.labels}
          onChange={(event) => patchDraft({ labels: event.target.value })}
        />
      </label>
      <label>
        Assignees
        <input
          value={current.assignees}
          onChange={(event) => patchDraft({ assignees: event.target.value })}
        />
      </label>
      <label className="work-detail__wide">
        Body
        <textarea
          value={current.body}
          onChange={(event) => patchDraft({ body: event.target.value })}
          rows={8}
        />
      </label>
      <button type="submit" disabled={pending || !current.title.trim()}>
        Save
      </button>
      {errorMessage ? (
        <p className="work-page__error">{errorMessage}</p>
      ) : null}
    </form>
  );
}
