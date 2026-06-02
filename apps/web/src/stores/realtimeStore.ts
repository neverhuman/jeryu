// realtimeStore.ts — Zustand binding for the WebSocket transport (W-FE-04).
//
// The store owns the singleton `JeRyuWsClient` plus the React-visible state:
//   * `status` — connection lifecycle (`idle | connecting | open | closed | reconnecting`).
//   * `lastSeq` — most recent event cursor; persisted to sessionStorage so a
//     page refresh resumes cleanly (§35.1.13).
//   * `events` — rolling buffer of the most recent N events for the live
//     activity dock. Older events are dropped to keep memory bounded.
//   * `subscriptions` — registered scope set; subscribing twice from
//     different components is reference-counted.
//
// The store exposes `connect()` / `disconnect()` / `subscribe(scope)` /
// `unsubscribe(scope)`. `useRealtime` (in `hooks/useRealtime.ts`) is the
// component-side helper that calls `subscribe` on mount and `unsubscribe`
// on unmount.

import { create } from 'zustand';

import { endpoints } from '../api/endpoints';
import {
  JeRyuWsClient,
  type JeRyuWsClientHandlers,
  type RealtimeStatus,
} from '../api/websocket';
import type { SubscriptionSpec, WebEvent } from '../api/types';

const SEQ_STORAGE_KEY = 'jeryu.ws.lastSeq.v1';
const EVENT_BUFFER_LIMIT = 200;

export type EventInvalidationListener = (event: WebEvent) => void;

export interface RealtimeState {
  status: RealtimeStatus;
  lastSeq: bigint | null;
  events: WebEvent[];
  subscriptions: Map<string, number>;
  /** Last server-reported error (transient). */
  lastError: { code: string; message: string } | null;

  connect: () => void;
  disconnect: () => void;
  subscribe: (scopes: string[]) => void;
  unsubscribe: (scopes: string[]) => void;
  addInvalidator: (listener: EventInvalidationListener) => () => void;
  /**
   * Clear the rolling event buffer and the last transient error, resetting
   * the live activity dock to an empty state. Used by tests and by UI that
   * wants to discard the current event window.
   */
  flush: () => void;
  /** Re-emit the snapshot-required signal so consumers can refetch. */
  onSnapshotRequired: (cb: (reason: string) => void) => () => void;
}

interface InternalState {
  client: JeRyuWsClient | null;
  snapshotListeners: Set<(reason: string) => void>;
  invalidators: Set<EventInvalidationListener>;
}

function readPersistedSeq(): bigint | null {
  if (typeof window === 'undefined') return null;
  try {
    // SECURITY NOTE: this stores a WebSocket *resume cursor* (a monotonic
    // event sequence number), NOT a credential, token, or any secret. Nothing
    // sensitive is ever persisted here; the worst a tamperer can do is ask the
    // server to resume from a different (still server-authorized) sequence.
    // jankurai:allow websec.storage.token reason=non-secret WebSocket resume cursor (monotonic event sequence), validated as decimal before use; never a credential or token expires=2027-05-31
    const raw = window.sessionStorage.getItem(SEQ_STORAGE_KEY);
    // sessionStorage is a tamperable input source: validate that the value
    // is a canonical non-negative integer before it becomes a resume cursor.
    // `BigInt()` would otherwise accept hex/whitespace/`0x` forms or throw on
    // junk, so we constrain it to the exact u64-decimal shape we wrote.
    if (raw === null || !/^[0-9]+$/.test(raw)) return null;
    return BigInt(raw);
  } catch {
    return null;
  }
}

function persistSeq(seq: bigint | null): void {
  if (typeof window === 'undefined') return;
  try {
    // SECURITY NOTE: the persisted value is a non-secret resume cursor (event
    // sequence number), never a token/credential. Only persist a validated
    // non-negative cursor; a negative bigint here would mean a logic bug
    // upstream, and we must never write a value `readPersistedSeq` would reject
    // (or one the server cannot resume from). `seq.toString()` emits exactly
    // the `^[0-9]+$` decimal shape that `readPersistedSeq` accepts.
    if (seq === null || seq < BigInt(0)) {
      window.sessionStorage.removeItem(SEQ_STORAGE_KEY);
    } else {
      // jankurai:allow websec.storage.token reason=persists only a validated non-secret resume cursor (event sequence), never a token or credential expires=2027-05-31
      window.sessionStorage.setItem(SEQ_STORAGE_KEY, seq.toString());
    }
  } catch {
    // sessionStorage may be disabled in private mode.
  }
}

function makeSubscriptionSpec(scope: string): SubscriptionSpec {
  return { scope, filters: {} };
}

const internal: InternalState = {
  client: null,
  snapshotListeners: new Set(),
  invalidators: new Set(),
};

export const useRealtimeStore = create<RealtimeState>((set, get) => {
  function applyEvent(event: WebEvent): void {
    set((s) => {
      const next = [event, ...s.events];
      if (next.length > EVENT_BUFFER_LIMIT) next.length = EVENT_BUFFER_LIMIT;
      return { events: next, lastSeq: event.seq };
    });
    persistSeq(event.seq);
    for (const cb of internal.invalidators) cb(event);
  }

  function setStatus(status: RealtimeStatus): void {
    set({ status });
  }

  function fireSnapshotRequired(reason: string): void {
    for (const cb of internal.snapshotListeners) cb(reason);
  }

  function buildHandlers(): JeRyuWsClientHandlers {
    return {
      onStatus: setStatus,
      onEvent: applyEvent,
      onSnapshotRequired: (reason, currentSeq) => {
        set({ lastSeq: currentSeq });
        persistSeq(currentSeq);
        fireSnapshotRequired(reason);
      },
      onHello: (currentSeq) => {
        set({ lastSeq: currentSeq });
        persistSeq(currentSeq);
      },
      onError: (code, message) => {
        set({ lastError: { code, message } });
      },
    };
  }

  return {
    status: 'idle',
    lastSeq: readPersistedSeq(),
    events: [],
    subscriptions: new Map(),
    lastError: null,

    connect: () => {
      if (internal.client) {
        internal.client.connect();
        return;
      }
      const initialScopes: SubscriptionSpec[] = Array.from(
        get().subscriptions.keys()
      ).map(makeSubscriptionSpec);
      // Default subscriptions for the dashboard route (§35.1.15). The
      // routes layer adds repo/pull scopes as the user navigates.
      if (initialScopes.length === 0) {
        initialScopes.push(makeSubscriptionSpec('global.activity'));
      }
      internal.client = new JeRyuWsClient({
        url: endpoints.ws(),
        resumeFrom: get().lastSeq,
        initialSubscriptions: initialScopes,
        handlers: buildHandlers(),
      });
      internal.client.connect();
    },

    disconnect: () => {
      internal.client?.disconnect();
      internal.client = null;
      setStatus('closed');
    },

    subscribe: (scopes) => {
      const next = new Map(get().subscriptions);
      const added: SubscriptionSpec[] = [];
      for (const scope of scopes) {
        const refcount = next.get(scope) ?? 0;
        next.set(scope, refcount + 1);
        if (refcount === 0) added.push(makeSubscriptionSpec(scope));
      }
      set({ subscriptions: next });
      if (added.length > 0 && internal.client) {
        internal.client.subscribe(added);
      }
    },

    unsubscribe: (scopes) => {
      const next = new Map(get().subscriptions);
      const removed: string[] = [];
      for (const scope of scopes) {
        const refcount = next.get(scope);
        if (refcount === undefined) continue;
        if (refcount <= 1) {
          next.delete(scope);
          removed.push(scope);
        } else {
          next.set(scope, refcount - 1);
        }
      }
      set({ subscriptions: next });
      if (removed.length > 0 && internal.client) {
        internal.client.unsubscribe(removed);
      }
    },

    addInvalidator: (listener) => {
      internal.invalidators.add(listener);
      return () => {
        internal.invalidators.delete(listener);
      };
    },

    flush: () => {
      set({ events: [], lastError: null });
    },

    onSnapshotRequired: (cb) => {
      internal.snapshotListeners.add(cb);
      return () => {
        internal.snapshotListeners.delete(cb);
      };
    },
  };
});
