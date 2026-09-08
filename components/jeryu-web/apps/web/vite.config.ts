import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:8787', ws: true },
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
        manualChunks: {
          'monaco-vendor': ['@monaco-editor/react'],
          'xterm-vendor': ['@xterm/xterm', '@xterm/addon-fit'],
          'markdown-vendor': [
            'react-markdown', 'remark-gfm', 'rehype-autolink-headings',
            'rehype-highlight', 'rehype-raw', 'rehype-sanitize',
            'rehype-slug', 'dompurify',
          ],
          'tanstack-vendor': [
            '@tanstack/react-query', '@tanstack/react-table',
            '@tanstack/react-virtual',
          ],
          'react-vendor': ['react', 'react-dom', 'react-router-dom'],
        },
      },
    },
  },
});
