// setup.ts — Vitest global setup (W-FE-08/09/10).
//
// Loaded automatically by Vitest via `vitest.config.ts → setupFiles`. We
// extend the `expect` matchers with @testing-library/jest-dom so tests can
// use `.toBeInTheDocument()` etc.

import '@testing-library/jest-dom/vitest';
