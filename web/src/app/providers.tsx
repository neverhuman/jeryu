// providers.tsx — global provider stack (W-FE-02 + W-CC-01 + W-CC-04).
//
// Order matters:
//   1. QueryClientProvider — Bootstrap and other server state.
//   2. ThemeProvider       — Applies the `data-theme` attribute to <html>
//                            from `preferencesStore.theme` + system color
//                            scheme.
//   3. KeyboardProvider    — Registers shortcut descriptors for the help
//                            overlay; does not own routing or stores.
//   4. RealtimeBoot        — Invisible component that connects the realtime
//                            WS client and bridges WS gaps to React Query.

import {
  QueryClient,
  QueryClientProvider,
  useQueryClient,
} from '@tanstack/react-query';
import {
  type ReactNode,
  useEffect,
  useMemo,
  useState,
} from 'react';

import { BOOTSTRAP_QUERY_KEY } from '../hooks/useBootstrap';
import { KeyboardProvider } from '../hooks/useKeyboard';
import { usePreferencesStore } from '../stores/preferencesStore';
import { useRealtimeStore } from '../stores/realtimeStore';

function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        retry: 1,
        refetchOnWindowFocus: false,
      },
      mutations: {
        retry: 0,
      },
    },
  });
}

/**
 * Theme provider: keeps `<html data-theme="…">` in sync with the user's
 * preference. `system` resolves via `prefers-color-scheme`.
 */
function ThemeProvider({ children }: { children: ReactNode }): JSX.Element {
  const theme = usePreferencesStore((s) => s.theme);

  useEffect(() => {
    const root = document.documentElement;
    const applyResolved = (mode: 'light' | 'dark') => {
      // `system` -> remove explicit override so :root + media query win.
      if (theme === 'system') {
        root.removeAttribute('data-theme');
        // We still set color-scheme so native widgets (scrollbars, form
        // controls) follow the system preference deterministically.
        root.style.colorScheme = mode;
        return;
      }
      root.setAttribute('data-theme', theme);
      root.style.colorScheme =
        theme === 'light' ? 'light' : 'dark';
    };
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    applyResolved(mq.matches ? 'dark' : 'light');
    const onChange = (e: MediaQueryListEvent): void => {
      applyResolved(e.matches ? 'dark' : 'light');
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [theme]);

  return <>{children}</>;
}

/**
 * Realtime boot: connects the WS client on mount and bridges gap events to
 * React Query so a `snapshot_required` triggers a bootstrap refetch.
 */
function RealtimeBoot(): null {
  const connect = useRealtimeStore((s) => s.connect);
  const disconnect = useRealtimeStore((s) => s.disconnect);
  const subscribeSnapshot = useRealtimeStore((s) => s.onSnapshotRequired);
  const queryClient = useQueryClient();

  useEffect(() => {
    connect();
    const offSnap = subscribeSnapshot(() => {
      queryClient.invalidateQueries({ queryKey: BOOTSTRAP_QUERY_KEY });
    });
    const offInvalidate = useRealtimeStore.getState().addInvalidator((evt) => {
      // Phase 1: invalidate the bootstrap query on any structural event
      // (`repo.*`, `pull.*`, `settings.*`, `agent.*`). Per-resource hooks
      // land in W-FE-06 with finer-grained invalidation maps.
      const kind = evt.kind || '';
      if (
        kind.startsWith('repo.') ||
        kind.startsWith('pull.') ||
        kind.startsWith('settings.') ||
        kind.startsWith('agent.')
      ) {
        queryClient.invalidateQueries({ queryKey: BOOTSTRAP_QUERY_KEY });
      }
    });
    return () => {
      offSnap();
      offInvalidate();
      disconnect();
    };
  }, [connect, disconnect, queryClient, subscribeSnapshot]);

  return null;
}

export function AppProviders({
  children,
}: {
  children: ReactNode;
}): JSX.Element {
  const [queryClient] = useState(createQueryClient);

  const inner = useMemo(
    () => (
      <ThemeProvider>
        <KeyboardProvider>
          <RealtimeBoot />
          {children}
        </KeyboardProvider>
      </ThemeProvider>
    ),
    [children]
  );

  return <QueryClientProvider client={queryClient}>{inner}</QueryClientProvider>;
}
