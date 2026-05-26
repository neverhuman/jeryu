import { defineConfig } from '@playwright/test';

// Minimal Playwright skeleton; W-T-08 will flesh out webServer wiring,
// projects (desktop/mobile), and trace/screenshot evidence collection.
export default defineConfig({
  testDir: './e2e',
  use: { baseURL: 'http://127.0.0.1:5173' },
});
