// useToolFinder.ts — data + live-scan hooks for the /tools page.
//
// Three surfaces over the system-wide tool-finder:
//   * `useToolFinderDashboard` — the persisted pattern-family dashboard.
//   * `useToolFinderScan` — the live scan controller: POST to start, status
//     streamed over the `tool_finder.scan` WebSocket scope (snapshot on
//     subscribe, throttled progress pushes), with a slow polling fallback
//     while a scan is running. Completion invalidates the dashboard +
//     registry queries so the page repaints with fresh families.
//   * `useIgnoreCluster` / `useProposeCluster` — per-cluster actions
//     (durable ignore feedback; promote into a jeryu-tool registry proposal).

import { useEffect, useMemo } from 'react';
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/react-query';

import { apiGet, apiSend } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  ToolFinderDashboard,
  ToolFinderProposeReceipt,
  ToolFinderScanStatus,
  WebEvent,
} from '../api/types';
import { useRealtime } from './useRealtime';
import { useRealtimeStore } from '../stores/realtimeStore';

/** The WebSocket scope the scan streams on (mirrors the server constant). */
export const TOOL_FINDER_SCAN_SCOPE = 'tool_finder.scan';

export function useToolFinderDashboard(): UseQueryResult<
  ToolFinderDashboard,
  Error
> {
  return useQuery({
    queryKey: ['tool-finder', 'dashboard'],
    queryFn: ({ signal }) =>
      apiGet<ToolFinderDashboard>(endpoints.toolFinderDashboard(), { signal }),
    staleTime: 30_000,
    retry: false,
  });
}

export interface ToolFinderScanController {
  /** Freshest status: the latest WS payload when newer than the GET snapshot. */
  status: ToolFinderScanStatus | undefined;
  isRunning: boolean;
  /** Scan-scope events, newest first, for the live activity feed. */
  feed: WebEvent[];
  start: () => void;
  isStarting: boolean;
  startError: Error | null;
}

function isScanStatus(payload: unknown): payload is ToolFinderScanStatus {
  return (
    typeof payload === 'object' &&
    payload !== null &&
    typeof (payload as ToolFinderScanStatus).phase === 'string' &&
    typeof (payload as ToolFinderScanStatus).running === 'boolean'
  );
}

export function useToolFinderScan(): ToolFinderScanController {
  const queryClient = useQueryClient();
  useRealtime([TOOL_FINDER_SCAN_SCOPE]);
  const events = useRealtimeStore((s) => s.events);

  const feed = useMemo(
    () => events.filter((event) => event.scope === TOOL_FINDER_SCAN_SCOPE),
    [events]
  );
  const wsStatus = useMemo(() => {
    const latest = feed[0];
    return latest && isScanStatus(latest.payload) ? latest.payload : undefined;
  }, [feed]);

  const wsRunning = wsStatus?.running ?? false;
  const startMutation = useMutation({
    mutationFn: () =>
      apiSend<ToolFinderScanStatus>(endpoints.toolFinderScan()),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['tool-finder', 'scan'] });
    },
  });
  const snapshot = useQuery({
    queryKey: ['tool-finder', 'scan'],
    queryFn: ({ signal }) =>
      apiGet<ToolFinderScanStatus>(endpoints.toolFinderScan(), { signal }),
    // Paint fallback + WS-down safety net; slow because WS is the live path.
    refetchInterval: wsRunning ? 3_000 : false,
    retry: false,
  });

  const startStatus = startMutation.data;
  const shouldUseStartStatus =
    startStatus?.running === true &&
    (!snapshot.data || snapshot.data.scan_id < startStatus.scan_id);

  const httpStatus = shouldUseStartStatus ? startStatus : snapshot.data;
  const wsIsFresh =
    wsStatus !== undefined &&
    (httpStatus === undefined || wsStatus.scan_id >= httpStatus.scan_id);

  // WS payload wins when it is at least as fresh as HTTP; otherwise keep the
  // POST/GET start status visible until a newer WS event arrives.
  const status = wsIsFresh ? wsStatus : httpStatus;

  // Completion/failure refreshes the dashboard, the scan snapshot, and the
  // registry summary (a completed scan can change candidate counts).
  const latestKind = feed[0]?.kind;
  useEffect(() => {
    if (
      latestKind === 'tool_finder.scan.completed' ||
      latestKind === 'tool_finder.scan.failed'
    ) {
      void queryClient.invalidateQueries({ queryKey: ['tool-finder'] });
      void queryClient.invalidateQueries({
        queryKey: ['tools', 'registry', 'summary'],
      });
    }
  }, [latestKind, queryClient]);

  return {
    status,
    isRunning: status?.running ?? false,
    feed,
    start: () => startMutation.mutate(),
    isStarting: startMutation.isPending,
    startError: startMutation.error,
  };
}

export function useIgnoreCluster(): UseMutationResult<
  unknown,
  Error,
  { clusterId: string; reason: string }
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ clusterId, reason }) =>
      apiSend(endpoints.toolBuildClusterFeedback(clusterId), {
        reason,
        ignored_by: 'web',
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ['tool-finder', 'dashboard'],
      });
    },
  });
}

export function useProposeCluster(): UseMutationResult<
  ToolFinderProposeReceipt,
  Error,
  { clusterId: string }
> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ clusterId }) =>
      apiSend<ToolFinderProposeReceipt>(endpoints.toolFinderPropose(clusterId)),
    onSuccess: () => {
      // A new proposal lands in the registry the left rail renders.
      void queryClient.invalidateQueries({
        queryKey: ['tools', 'registry', 'summary'],
      });
    },
  });
}
