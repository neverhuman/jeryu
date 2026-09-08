import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { FormEvent } from 'react';
import { useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { apiGet, apiPatch, apiSend } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  CreateWorkCommentRequest,
  CreateWorkLinkRequest,
  UpdateWorkItemRequest,
  WorkComment,
  WorkItem,
  WorkItemDetail,
} from '../api/types';
import {
  csvTokens,
  emptyEdit,
  principalsFromInput,
  type EditState,
} from './workModel';
import {
  WorkDetailComments,
  WorkDetailEditForm,
  WorkDetailHeader,
  WorkDetailSidebar,
} from './work';

import './page.css';
import './WorkPage.css';

export function WorkDetailPage(): JSX.Element {
  const params = useParams();
  const key = params.key ?? '';
  const queryClient = useQueryClient();
  const [comment, setComment] = useState('');
  const [pullOwner, setPullOwner] = useState('');
  const [pullRepo, setPullRepo] = useState('');
  const [pullNumber, setPullNumber] = useState('');

  const detail = useQuery({
    queryKey: ['work-detail', key],
    queryFn: ({ signal }) =>
      apiGet<WorkItemDetail>(endpoints.workItem(key), { signal }),
    enabled: key.length > 0,
    staleTime: 10_000,
  });

  const edit = useMemo<EditState | null>(() => {
    const item = detail.data?.item;
    if (!item) return null;
    return {
      title: item.title,
      body: item.body ?? '',
      status: item.status,
      kind: item.kind,
      priority: item.priority,
      labels: item.labels.join(', '),
      assignees: item.assignees
        .map((assignee) =>
          assignee.kind === 'agent' ? `agent:${assignee.id}` : assignee.id
        )
        .join(', '),
    };
  }, [detail.data]);
  const [draft, setDraft] = useState<EditState | null>(null);
  const current = draft ?? edit;

  const saveItem = useMutation({
    mutationFn: (request: UpdateWorkItemRequest) =>
      apiPatch<WorkItem>(endpoints.workItem(key), request),
    onSuccess: async () => {
      setDraft(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['work-detail', key] }),
        queryClient.invalidateQueries({ queryKey: ['work'] }),
        queryClient.invalidateQueries({ queryKey: ['repo-work'] }),
      ]);
    },
  });

  const addComment = useMutation({
    mutationFn: (request: CreateWorkCommentRequest) =>
      apiSend<WorkComment>(endpoints.workComments(key), request),
    onSuccess: async () => {
      setComment('');
      await queryClient.invalidateQueries({ queryKey: ['work-detail', key] });
    },
  });

  const addLink = useMutation({
    mutationFn: (request: CreateWorkLinkRequest) =>
      apiSend<WorkItem>(endpoints.workLinks(key), request),
    onSuccess: async () => {
      setPullOwner('');
      setPullRepo('');
      setPullNumber('');
      await queryClient.invalidateQueries({ queryKey: ['work-detail', key] });
    },
  });

  function patchDraft(change: Partial<EditState>): void {
    setDraft({ ...(current ?? emptyEdit()), ...change });
  }

  function submitSave(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!current) return;
    saveItem.mutate({
      title: current.title.trim(),
      body: current.body.trim() || null,
      status: current.status,
      kind: current.kind,
      priority: current.priority,
      labels: csvTokens(current.labels),
      assignees: principalsFromInput(current.assignees),
    });
  }

  function submitComment(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const body = comment.trim();
    if (!body) return;
    addComment.mutate({ body, author: null });
  }

  function submitPullLink(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const owner = pullOwner.trim();
    const repo = pullRepo.trim();
    const parsed = Number.parseInt(pullNumber.trim(), 10);
    if (!owner || !repo || !Number.isFinite(parsed) || parsed < 1) return;
    addLink.mutate({
      issue: null,
      pull_request: {
        owner,
        repo,
        number: parsed,
        url: `/repos/jeryu/${owner}/${repo}/pulls/${parsed}`,
      },
    });
  }

  if (detail.isPending) {
    return (
      <div className="page" data-testid="work-detail-page">
        <p className="page__roadmap-note">Loading work item.</p>
      </div>
    );
  }

  if (detail.isError || !detail.data || !current) {
    return (
      <div className="page" data-testid="work-detail-page">
        <header className="page__header">
          <h1 className="page__title">Work</h1>
        </header>
        <p className="page__roadmap-note">
          {detail.error?.message ?? 'Work item not found.'}
        </p>
      </div>
    );
  }

  const item = detail.data.item;

  return (
    <div className="page page--full work-detail" data-testid="work-detail-page">
      <WorkDetailHeader item={item} />

      <div className="work-detail__grid">
        <WorkDetailEditForm
          current={current}
          patchDraft={patchDraft}
          onSubmit={submitSave}
          pending={saveItem.isPending}
          errorMessage={saveItem.isError ? saveItem.error.message : null}
        />
        <WorkDetailSidebar
          item={item}
          pullOwner={pullOwner}
          pullRepo={pullRepo}
          pullNumber={pullNumber}
          setPullOwner={setPullOwner}
          setPullRepo={setPullRepo}
          setPullNumber={setPullNumber}
          onSubmit={submitPullLink}
          pending={addLink.isPending}
          errorMessage={addLink.isError ? addLink.error.message : null}
        />
      </div>

      <WorkDetailComments
        comments={detail.data.comments}
        comment={comment}
        setComment={setComment}
        onSubmit={submitComment}
        pending={addComment.isPending}
        errorMessage={addComment.isError ? addComment.error.message : null}
      />
    </div>
  );
}
