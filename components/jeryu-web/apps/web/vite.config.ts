import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: process.env.JERYU_PLAYWRIGHT_E2E_MODE === 'ui-only' ? {} : {
      '/api': { target: process.env.JERYU_PLAYWRIGHT_API_URL ?? 'http://127.0.0.1:8787', ws: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    // Manual chunking keeps the main entry under Vite's 500 KB
    // warning threshold by splitting the three large vendor surfaces
    // (Monaco editor, markdown pipeline, TanStack data layer) into
    // their own lazily-evaluated chunks.
    rollupOptions: {
      output: {
        // Vite 8 / Rollup 4 accept only the function form of manualChunks.
        // Keep the same vendor groups as the previous object form.
        manualChunks(id) {
          if (id.includes('monaco-editor') || id.includes('@monaco-editor')) {
            return 'monaco-vendor';
          }
          if (id.includes('@xterm/')) {
            return 'xterm-vendor';
          }
          if (
            id.includes('react-markdown') ||
            id.includes('remark-gfm') ||
            id.includes('rehype-') ||
            id.includes('/dompurify/')
          ) {
            return 'markdown-vendor';
          }
          if (id.includes('@tanstack/')) {
            return 'tanstack-vendor';
          }
          if (
            id.includes('/node_modules/react/') ||
            id.includes('/node_modules/react-dom/') ||
            id.includes('react-router-dom')
          ) {
            return 'react-vendor';
          }
          return undefined;
        },
      },
    },
  },
});
