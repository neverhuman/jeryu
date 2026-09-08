// GoldenToolBox.test.tsx — render + degradation smoke for the gold box.
//
// The box must (a) render the headline metrics, the status breakdown, the top
// three tools, and a link to the tool control-plane repo when the registry has
// tools, and (b) degrade to nothing on loading / error / empty / missing data
// so it can never break the repositories page. The query hook is mocked so the
// test does not depend on a live `/api/v1/tools/registry/summary`.

import type { UseQueryResult } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RETIRED_STATUS_LABEL } from '../toolStatusProductCopy';
import { GoldenToolBox } from '../GoldenToolBox';
import type { ToolRegistrySummary } from '../../../api/types';

const useToolRegistryMock = vi.fn();

vi.mock('../../../hooks/useToolRegistry', () => ({
  useToolRegistry: () => useToolRegistryMock(),
}));

type Result = Partial<UseQueryResult<ToolRegistrySummary, Error>>;

function setHookResult(result: Result): void {
  useToolRegistryMock.mockReturnValue({
    isLoading: false,
    isError: false,
    data: undefined,
    ...result,
  });
}

const SUMMARY: ToolRegistrySummary = {
  generated_at: '2026-06-12T10:00:00Z',
  tool_count: 4,
  published_count: 2,
  building_count: 1,
  proposed_count: 1,
  deprecated_count: 0,
  adopting_repo_count: 7,
  candidate_repo_count: 3,
  open_task_count: 5,
  realized_loc_saved: 12840,
  anticipated_loc_saved: 4200,
  tools: [
    {
      id: 't-1',
      name: 'forced-diff-scorer',
      kind: 'library',
      status: 'published',
      adopting_repo_count: 5,
      candidate_repo_count: 1,
      loc_saved: 8200,
      loc_saved_estimate: 9000,
    },
    {
      id: 't-2',
      name: 'jankurai-gate',
      kind: 'cli',
      status: 'building',
      adopting_repo_count: 2,
      candidate_repo_count: 1,
      loc_saved: 0,
      loc_saved_estimate: 3100,
    },
    {
      id: 't-3',
      name: 'mirror-sync',
      kind: 'service',
      status: 'proposed',
      adopting_repo_count: 0,
      candidate_repo_count: 1,
      loc_saved: 0,
      loc_saved_estimate: 1200,
    },
    {
      id: 't-4',
      name: 'never-shown',
      kind: 'library',
      status: 'published',
      adopting_repo_count: 1,
      candidate_repo_count: 0,
      loc_saved: 400,
      loc_saved_estimate: 400,
    },
  ],
};

afterEach(() => {
  useToolRegistryMock.mockReset();
});

describe('GoldenToolBox', () => {
  it('renders headline metrics, status breakdown, and the control-plane link', () => {
    setHookResult({ data: SUMMARY });
    render(
      <MemoryRouter>
        <GoldenToolBox />
      </MemoryRouter>
    );

    const link = screen.getByRole('link', {
      name: /jeryu-tool control plane \(4 reusable tools\)/,
    });
    expect(link).toHaveAttribute('href', '/tools');
    expect(link).toHaveClass('repo-golden-box');

    expect(screen.getByText('Tool control plane')).toBeInTheDocument();
    // Status breakdown drops zero-count states (the retired lifecycle).
    expect(screen.getByText('2 published')).toBeInTheDocument();
    expect(screen.getByText('1 building')).toBeInTheDocument();
    expect(screen.getByText('1 proposed')).toBeInTheDocument();
    expect(
      screen.queryByText(new RegExp(RETIRED_STATUS_LABEL))
    ).not.toBeInTheDocument();

    expect(screen.getByText('7')).toBeInTheDocument(); // adopting repos
    expect(screen.getByText('5')).toBeInTheDocument(); // open tasks
    expect(screen.getByText(/\+4,200 anticipated/)).toBeInTheDocument();
  });

  it('lists only the top three tools, with realized vs estimated LOC', () => {
    setHookResult({ data: SUMMARY });
    render(
      <MemoryRouter>
        <GoldenToolBox />
      </MemoryRouter>
    );

    expect(screen.getByText('forced-diff-scorer')).toBeInTheDocument();
    expect(screen.getByText('jankurai-gate')).toBeInTheDocument();
    expect(screen.getByText('mirror-sync')).toBeInTheDocument();
    expect(screen.queryByText('never-shown')).not.toBeInTheDocument();

    // Realized savings render bare; not-yet-realized fall back to the estimate
    // prefixed with `~`.
    expect(screen.getByText('8,200 LOC')).toBeInTheDocument();
    expect(screen.getByText('~3,100 LOC')).toBeInTheDocument();
  });

  it('degrades to nothing while loading, on error, when empty, or with no data', () => {
    for (const result of [
      { isLoading: true } as Result,
      { isError: true } as Result,
      { data: { ...SUMMARY, tool_count: 0, tools: [] } } as Result,
      { data: undefined } as Result,
    ]) {
      setHookResult(result);
      const { container, unmount } = render(
        <MemoryRouter>
          <GoldenToolBox />
        </MemoryRouter>
      );
      expect(container).toBeEmptyDOMElement();
      unmount();
    }
  });
});
