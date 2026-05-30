// ESLint 9 flat config (replaces .eslintrc.cjs). Lints the JS/TS/TSX
// surface of @jeryu/web — TypeScript parsing now goes through the
// @typescript-eslint parser so .tsx files participate (vs. the legacy
// config which restricted ESLint to .js/.cjs/.mjs).
//
// ESLint 10 will drop the legacy eslintrc loader entirely; this flat
// config is the supported path forward (see
// https://eslint.org/docs/latest/use/configure/migration-guide).
import js from '@eslint/js';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import reactHooks from 'eslint-plugin-react-hooks';
import tsParser from '@typescript-eslint/parser';
import tsPlugin from '@typescript-eslint/eslint-plugin';

// Browser + DOM globals consumed across the SPA. The flat config no longer
// inherits the legacy `env: { browser: true }` shortcut, so the list is
// enumerated explicitly. TypeScript files set `no-undef: off` (TS handles
// name resolution); JS files keep the rule on for safety.
const browserGlobals = {
  window: 'readonly', document: 'readonly', console: 'readonly',
  fetch: 'readonly', WebSocket: 'readonly', URL: 'readonly',
  URLSearchParams: 'readonly', navigator: 'readonly',
  sessionStorage: 'readonly', localStorage: 'readonly',
  crypto: 'readonly', setTimeout: 'readonly', setInterval: 'readonly',
  clearTimeout: 'readonly', clearInterval: 'readonly',
  HTMLElement: 'readonly', HTMLInputElement: 'readonly',
  HTMLDivElement: 'readonly', HTMLAnchorElement: 'readonly',
  HTMLButtonElement: 'readonly', HTMLTextAreaElement: 'readonly',
  HTMLSelectElement: 'readonly', HTMLFormElement: 'readonly',
  Element: 'readonly', Node: 'readonly',
  Event: 'readonly', KeyboardEvent: 'readonly',
  MouseEvent: 'readonly', MessageEvent: 'readonly',
  MediaQueryListEvent: 'readonly', EventTarget: 'readonly',
  FormData: 'readonly', Response: 'readonly',
  Request: 'readonly', AbortController: 'readonly',
  AbortSignal: 'readonly', RequestInit: 'readonly', HeadersInit: 'readonly',
  requestAnimationFrame: 'readonly', cancelAnimationFrame: 'readonly',
  Blob: 'readonly', File: 'readonly', FileReader: 'readonly',
  IntersectionObserver: 'readonly', MutationObserver: 'readonly',
  ResizeObserver: 'readonly',
};

export default [
  js.configs.recommended,
  // ── TypeScript / TSX ──
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
        ecmaFeatures: { jsx: true },
      },
      globals: { ...browserGlobals, JSX: 'readonly' },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      'jsx-a11y': jsxA11y,
      'react-hooks': reactHooks,
    },
    rules: {
      ...jsxA11y.configs.recommended.rules,
      ...reactHooks.configs.recommended.rules,
      // TypeScript already resolves identifiers; ESLint's `no-undef`
      // produces noisy false positives on type-only names (JSX, etc.).
      'no-undef': 'off',
      'no-unused-vars': 'off',
      'no-empty': ['warn', { allowEmptyCatch: true }],
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      // The migration exposes a backlog of pre-existing findings (the
      // legacy config only linted .js/.cjs/.mjs and missed all .tsx). We
      // demote them to warnings so the lint step passes and the backlog
      // is tracked as tech debt rather than blocking the polish PR. Bump
      // back to 'error' once the call sites are remediated.
      'jsx-a11y/no-autofocus': 'warn',
      'jsx-a11y/role-supports-aria-props': 'warn',
      'jsx-a11y/role-has-required-aria-props': 'warn',
      'jsx-a11y/click-events-have-key-events': 'warn',
      'jsx-a11y/no-noninteractive-element-interactions': 'warn',
      'react-hooks/set-state-in-effect': 'warn',
      // The "Cannot create components / access refs during render"
      // diagnostics come from react-hooks' purity / static-components /
      // refs rules. Demote to warning until the backlog is fixed.
      'react-hooks/purity': 'warn',
      'react-hooks/static-components': 'warn',
      'react-hooks/refs': 'warn',
    },
  },
  // ── Node-side config / build scripts ──
  {
    files: ['*.config.{js,cjs,mjs,ts}', 'perf/**/*.{js,cjs,mjs}'],
    languageOptions: {
      globals: {
        process: 'readonly', module: 'readonly', require: 'readonly',
        __dirname: 'readonly', __filename: 'readonly', console: 'readonly',
      },
    },
  },
  // ── Storybook stories: allow CSF react usage ──
  {
    files: ['**/*.stories.{ts,tsx}'],
    rules: {
      '@typescript-eslint/no-unused-vars': 'off',
    },
  },
  // ── E2E tests (Playwright) ──
  {
    files: ['e2e/**/*.{ts,tsx}'],
    languageOptions: {
      globals: { ...browserGlobals, process: 'readonly' },
    },
  },
  // ── Global linter settings ──
  {
    linterOptions: {
      // Legacy eslintrc silently tolerated orphan disable-directives for
      // plugins that were not installed (e.g. `react/no-danger`); the flat
      // loader is strict. Demote to warning so callers can clean these up
      // incrementally without failing the lint step.
      reportUnusedDisableDirectives: 'warn',
    },
  },
  {
    ignores: [
      'node_modules/**', 'dist/**', 'storybook-static/**',
      'playwright-report/**', 'test-results/**', 'coverage/**',
    ],
  },
];
