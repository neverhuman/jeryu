// Storybook preview wrapper for the JeRyu Web Forge SPA (W-T-07).
//
// We:
//   1. Pull in the SPA's design tokens + global styles so components
//      render with the same vocabulary they do in production.
//   2. Wrap every story in `<MemoryRouter>` because many components use
//      `<Link>` and would throw outside a router context.
//   3. Wrap stories in a `QueryClientProvider` for components that use
//      `useQuery` (search, bootstrap, etc.).
//   4. Default the a11y addon to "todo" so stories with intentionally
//      degraded states (loading, error, empty) still register but do
//      not fail the build; serious/critical violations are still
//      surfaced in the panel.

import type { Preview } from '@storybook/react-vite';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';

import '../src/styles/tokens.css';
import '../src/styles/app.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      staleTime: Infinity,
      // Storybook is a static render harness; never refetch.
      refetchOnWindowFocus: false,
    },
  },
});

const preview: Preview = {
  parameters: {
    layout: 'padded',
    a11y: {
      // Match the production-grade thresholds the UX-QA harness uses —
      // anything `serious`/`critical` is a real bug.
      config: {
        rules: [],
      },
      options: {
        runOnly: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'],
      },
    },
  },
  decorators: [
    (Story, ctx) => (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter
          initialEntries={[ctx.parameters?.router?.initialEntry ?? '/']}
        >
          <div className="sb-padding" style={{ padding: 16 }}>
            <Story />
          </div>
        </MemoryRouter>
      </QueryClientProvider>
    ),
  ],
};

export default preview;
