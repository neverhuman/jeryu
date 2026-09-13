import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';

import { apiSend } from '../../../api/client';
import type { RepositorySummary } from '../../../api/types';
import { useCreateRepoDialog } from '../useCreateRepoDialog';

vi.mock('../../../api/client', async (original) => ({
  ...await original<typeof import('../../../api/client')>(),
  apiSend: vi.fn(),
}));

afterEach(() => vi.resetAllMocks());

it('sends one in-flight creation and releases its key after acknowledged completion', async () => {
  let finish: ((repo: RepositorySummary) => void) | undefined;
  const pending = new Promise<RepositorySummary>((resolve) => { finish = resolve; });
  vi.mocked(apiSend).mockReturnValue(pending);
  const onCreated = vi.fn();
  const { result } = renderHook(() => useCreateRepoDialog({
    open: true, onCancel: vi.fn(), onCreated, defaultHost: 'jeryu', defaultOwner: 'alice',
  }));
  act(() => result.current.setDraft((draft) => ({ ...draft, name: 'first' })));
  let first: Promise<void> | undefined;
  act(() => {
    first = result.current.handleCreate();
    void result.current.handleCreate();
  });
  expect(apiSend).toHaveBeenCalledTimes(1);
  const initialKey = vi.mocked(apiSend).mock.calls[0][2]?.idempotencyKey;
  expect(initialKey).toMatch(/^[a-zA-Z0-9-]{16,128}$/);
  await act(async () => {
    finish?.({} as RepositorySummary);
    await first;
  });
  expect(onCreated).toHaveBeenCalledTimes(1);
  await act(async () => { await result.current.handleCreate(); });
  expect(apiSend).toHaveBeenCalledTimes(2);
  expect(vi.mocked(apiSend).mock.calls[1][2]?.idempotencyKey).not.toBe(initialKey);
});
