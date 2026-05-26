// ESLint legacy (eslintrc) config — consumed by ESLint v9 when run with
// ESLINT_USE_FLAT_CONFIG=false (see `lint` script in package.json). Note:
//   * Legacy config support is removed in ESLint v10; W-FE-* may migrate
//     to eslint.config.js (flat config) along with adding
//     @typescript-eslint/parser to lint .ts/.tsx files.
//   * The `lint` script currently restricts ESLint to .js/.cjs/.mjs files
//     because TS/TSX parsing needs @typescript-eslint/parser which is
//     out of scope for this foundation skeleton (W-F-07).
/** @type {import('eslint').Linter.LegacyConfig} */
module.exports = {
  root: true,
  env: { browser: true, es2022: true, node: true },
  parserOptions: {
    ecmaVersion: 2022,
    sourceType: 'module',
    ecmaFeatures: { jsx: true },
  },
  settings: { react: { version: '19' } },
  extends: [
    'eslint:recommended',
    'plugin:jsx-a11y/recommended',
    'plugin:react-hooks/recommended',
  ],
  plugins: ['jsx-a11y', 'react-hooks'],
  ignorePatterns: ['dist/', 'node_modules/', 'storybook-static/'],
};
