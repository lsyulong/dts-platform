/**
 * CI / standalone config: use when Vite dev server is already running.
 * Start the dev server first:
 *   pnpm dev
 * Then run:
 *   npx playwright test -c e2e/playwright.ci.config.ts
 */
import { defineConfig, devices } from '@playwright/test';

const PORT = 5173;

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  retries: 0,
  reporter: [['list'], ['html', { outputFolder: 'playwright-report' }]],
  use: {
    baseURL: `http://localhost:${PORT}`,
    locale: 'zh-CN',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
});
