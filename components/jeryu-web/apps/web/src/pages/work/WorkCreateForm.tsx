import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { FormEvent } from 'react';
import { useState } from 'react';

import { apiSend } from '../../api/client';
import type {
  CreateWorkItemRequest,
  WorkItem,
  WorkItemKind,
  WorkPriority,
  WorkStatus,
} from '../../api/types';
import {
  WORK_KINDS,
  WORK_KIND_LABELS,
  WORK_PRIORITIES,
  WORK_PRIORITY_LABELS,
  WORK_STATUSES,
  WORK_STATUS_LABELS,
  csvTokens,
  principalsFromInput,
} from '../workModel';

interface CreateFormState {
  title: string;
  body: string;
  kind: WorkItemKind;
  priority: WorkPriority;
  status: WorkStatus;
  labels: string;
  assignees: string;
}

const DEFAULT_CREATE_FORM: CreateFormState = {
  title: '',
  body: '',
  kind: 'task',
  priority: 'p2',
  status: 'ready',
  labels: '',
  assignees: '',
};

export interface WorkCreateFormProps {
  createUrl: string;
  queryKey: readonly unknown[];
  repoScoped: boolean;
}

export function WorkCreateForm({
  createUrl,
  queryKey,
  repoScoped,
}: WorkCreateFormProps): JSX.Element {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<CreateFormState>(DEFAULT_CREATE_FORM);

  const create = useMutation({
    mutationFn: (request: CreateWorkItemRequest) =>
      apiSend<WorkItem>(createUrl, request),
    onSuccess: async () => {
      setForm(DEFAULT_CREATE_FORM);
      await queryClient.invalidateQueries({ queryKey });
      if (repoScoped) {
        await queryClient.invalidateQueries({ queryKey: ['work'] });
      }
    },
  });

  function submitCreate(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const titleValue = form.title.trim();
    if (!titleValue) return;
    create.mutate({
      repo: null,
      title: titleValue,
      body: form.body.trim() || null,
      status: form.status,
      kind: form.kind,
      priority: form.priority,
      labels: csvTokens(form.labels),
      assignees: principalsFromInput(form.assignees),
    });
  }

  return (
    <section className="work-page__create" aria-label="Create work item">
      <form className="work-create" onSubmit={submitCreate}>
        <label className="work-create__title">
          Title
          <input
            value={form.title}
            onChange={(event) =>
              setForm((current) => ({ ...current, title: event.target.value }))
            }
            placeholder="New work item"
            required
          />
        </label>
        <label>
          Kind
          <select
            value={form.kind}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                kind: event.target.value as WorkItemKind,
              }))
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
            value={form.priority}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                priority: event.target.value as WorkPriority,
              }))
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
          Status
          <select
            value={form.status}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                status: event.target.value as WorkStatus,
              }))
            }
          >
            {WORK_STATUSES.map((status) => (
              <option key={status} value={status}>
                {WORK_STATUS_LABELS[status]}
              </option>
            ))}
          </select>
        </label>
        <label className="work-create__body">
          Body
          <textarea
            value={form.body}
            onChange={(event) =>
              setForm((current) => ({ ...current, body: event.target.value }))
            }
            rows={2}
          />
        </label>
        <label>
          Labels
          <input
            value={form.labels}
            onChange={(event) =>
              setForm((current) => ({ ...current, labels: event.target.value }))
            }
            placeholder="comma,separated"
          />
        </label>
        <label>
          Assignees
          <input
            value={form.assignees}
            onChange={(event) =>
              setForm((current) => ({ ...current, assignees: event.target.value }))
            }
            placeholder="alice,agent:runner"
          />
        </label>
        <button
          type="submit"
          className="work-create__submit"
          disabled={create.isPending || !form.title.trim()}
        >
          Create
        </button>
      </form>
      {create.isError ? (
        <p className="work-page__error">{create.error.message}</p>
      ) : null}
    </section>
  );
}
