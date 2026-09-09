// websocket.types.ts — public type surface for the realtime WebSocket client.
//
// These declarations are consumed by `JeRyuWsClient` (`./websocket.client`)
// and re-exported through the `./websocket` entry point so app code keeps a
// single import boundary. Keeping them here lets the client module stay
// focused on the socket lifecycle (open/close/reconnect/heartbeat).

import type { SubscriptionSpec, WebEvent } from './types';

/** A tap receives `event` frames for a single scope before they reach the
 *  zustand buffer. Used by the agent-terminal transport so high-frequency TTY
 *  bytes never pass through the rolling 200-event buffer. */
export type ScopeTap = (event: WebEvent) => void;

export type RealtimeStatus =
  | 'idle'
  | 'connecting'
  | 'open'
  | 'closed'
  | 'reconnecting';

export interface JeRyuWsClientHandlers {
  onStatus: (status: RealtimeStatus) => void;
  onEvent: (event: WebEvent) => void;
  /**
   * Fired whenever the server announces an out-of-band gap (either through
   * `SnapshotRequired` or because the resumed-from cursor is older than
   * `current_seq - 1`). The store responds by invalidating the bootstrap
   * query so React Query refetches the read model.
   */
  onSnapshotRequired: (reason: string, currentSeq: bigint) => void;
  onError?: (code: string, message: string) => void;
  /** Called when the server's `Hello` frame arrives, with `current_seq`. */
  onHello?: (currentSeq: bigint) => void;
}

export interface JeRyuWsClientOptions {
  /** Absolute or relative WS URL. Relative paths are resolved against
   *  `window.location`. The default `/api/v1/ws` matches `endpoints.ws()`. */
  url: string;
  /** Initial subscription set sent in the `Hello` frame. */
  initialSubscriptions: SubscriptionSpec[];
  /** Resume cursor; nullable on cold start. */
  resumeFrom: bigint | null;
  handlers: JeRyuWsClientHandlers;
}
