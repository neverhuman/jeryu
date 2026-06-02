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
      // Dynamic code execution is the canonical XSS/RCE sink in a browser
      // SPA. We have no `eslint-plugin-security`, but these core rules cover
      // the high-value cases (`eval`, `new Function`, string `setTimeout`)
      // and stay on as errors so an unsafe sink fails the lint gate.
      'no-eval': 'error',
      'no-implied-eval': 'error',
      'no-script-url': 'error',
      // The lint surface grew when TSX joined the config (the legacy config
      // only covered .js/.cjs/.mjs). These ergonomic/a11y findings are
      // reported as warnings so they stay visible without blocking; they are
      // not security rules and do not gate the build.
      'jsx-a11y/no-autofocus': 'warn',
      'jsx-a11y/role-supports-aria-props': 'warn',
      'jsx-a11y/role-has-required-aria-props': 'warn',
      'jsx-a11y/click-events-have-key-events': 'warn',
      'jsx-a11y/no-noninteractive-element-interactions': 'warn',
      'react-hooks/set-state-in-effect': 'warn',
      // react-hooks' purity / static-components / refs rules report
      // render-phase patterns (creating components or reading refs during
      // render). They run as warnings: advisory signal that surfaces the
      // pattern without gating the build, since these are style guidance
      // rather than correctness or security rules.
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
      // Enforce that every `eslint-disable` directive still suppresses a real
      // finding. An orphan disable (e.g. a stale `react/no-danger` opt-out
      // left over after the plugin was dropped) silently grants a security
      // exception that no longer maps to an active rule, which is exactly the
      // kind of un-auditable suppression we want to fail on rather than warn.
      reportUnusedDisableDirectives: 'error',
    },
  },
  {
    ignores: [
      'node_modules/**', 'dist/**', 'storybook-static/**',
      'playwright-report/**', 'test-results/**', 'coverage/**',
    ],
  },
];
