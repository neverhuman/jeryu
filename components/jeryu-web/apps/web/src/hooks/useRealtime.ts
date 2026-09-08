// useRealtime.ts — component-side subscription helper (W-FE-04).
//
// Components call `useRealtime(['repo.foo', 'pull.42'])` to subscribe to a set
// of scopes for the duration of their mount. The store reference-counts so
// multiple components asking for the same scope share a single
// subscribe/unsubscribe round trip.

import { useEffect } from "react";

import { useRealtimeStore } from '../stores/realtimeStore';

export function useRealtime(scopes: string[]): void {
  // Stabilize via a PRIMITIVE STRING key so callers can pass fresh array
  // literals (`useRealtime(['repo.foo'])`) without an infinite resubscribe
  // loop. React's referential equality on the primitive string drives
  // useEffect deps cleanly; the array path triggers a loop because each
  // render creates a new array reference even when contents are identical.
  // Audit finding from Phase 1 retro.
  const stableKey = scopes.length === 0 ? '' : [...new Set(scopes)].sort().join('|');

  useEffect(() => {
    if (!stableKey) return () => {};
    const stable = stableKey.split('|');
    const { subscribe, unsubscribe } = useRealtimeStore.getState();
    subscribe(stable);
    return () => unsubscribe(stable);
  }, [stableKey]);
}
