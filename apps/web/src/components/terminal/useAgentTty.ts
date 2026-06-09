// useAgentTty.ts — mount-scoped subscription to a single agent run's live TTY.
//
// Wraps `realtimeStore.subscribeTty` so a terminal component subscribes to
// `agent_run.{id}` on mount and tears the subscription (and its transport tap)
// down on unmount. Each tapped `WebEvent` is validated + decoded into an
// `AgentTtyFrame` here so the consumer only ever sees well-formed frames; junk
// payloads are dropped at this boundary.

import { useEffect, useRef } from 'react';

import type { AgentTtyFrame, WebEvent } from '../../api/types';
import { useRealtimeStore } from '../../stores/realtimeStore';
import { parseTtyFrame } from './agentTtyDecode';

/** Consumer callback: receives each validated frame plus the raw event (for
 *  the seq cursor / reconnect bookkeeping the caller may want). */
export type AgentTtyFrameHandler = (
  frame: AgentTtyFrame,
  event: WebEvent
) => void;

/**
 * Subscribe to `runId`'s live TTY for the lifetime of the calling component.
 * `onFrame` is held in a ref so callers can pass a fresh closure each render
 * without resubscribing; the subscription only churns when `runId` changes.
 */
export function useAgentTty(
  runId: string,
  onFrame: AgentTtyFrameHandler
): void {
  const handlerRef = useRef(onFrame);
  useEffect(() => {
    handlerRef.current = onFrame;
  }, [onFrame]);

  const subscribeTty = useRealtimeStore((s) => s.subscribeTty);

  useEffect(() => {
    if (!runId) return undefined;
    const unsubscribe = subscribeTty(runId, (event) => {
      const frame = parseTtyFrame(event.payload);
      if (frame) handlerRef.current(frame, event);
    });
    return unsubscribe;
  }, [runId, subscribeTty]);
}
