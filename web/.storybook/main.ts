// Storybook configuration for the JeRyu Web Forge SPA (W-T-07).
//
// Framework: `@storybook/react-vite` (Vite-driven; matches `vite.config.ts`).
// Addons: `@storybook/addon-a11y` (axe-core panel, every story is scanned),
//         `@storybook/addon-vitest` (vitest runner integration).
//
// Stories live next to their components as `*.stories.tsx`, so `glob`
// matches anywhere under `../src` to pick them up no matter how deep the
// component tree goes.

import type { StorybookConfig } from '@storybook/react-vite';

const config: StorybookConfig = {
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  stories: ['../src/**/*.stories.@(ts|tsx|mdx)'],
  addons: ['@storybook/addon-a11y', '@storybook/addon-vitest'],
  typescript: {
    check: false,
    reactDocgen: false,
  },
  docs: {
    autodocs: false,
  },
};

export default config;
