// useRealtime.ts — component-side subscription helper (W-FE-04).
//
// Components call `useRealtime(['repo.foo', 'mr.42'])` to subscribe to a set
// of scopes for the duration of their mount. The store reference-counts so
// multiple components asking for the same scope share a single
// subscribe/unsubscribe round trip.

import { useEffect, useMemo } from 'react';

import { useRealtimeStore } from '../stores/realtimeStore';

export function useRealtime(scopes: string[]): void {
  // Stabilize the array reference so we only resubscribe when the scope set
  // actually changes (not on every render).
  const stable = useMemo(() => {
    const sorted = [...new Set(scopes)].sort();
    return sorted;
  }, [scopes]);

  useEffect(() => {
    if (stable.length === 0) return undefined;
    const { subscribe, unsubscribe } = useRealtimeStore.getState();
    subscribe(stable);
    return () => unsubscribe(stable);
  }, [stable]);
}
