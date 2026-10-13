import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: 'pipeline-mvp.spec.ts',
  outputDir: '../test-results/pipeline-smoke',
  workers: 1,
  reporter: 'list',
  use: { baseURL: 'http://127.0.0.1:5173', locale: 'zh-CN' },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } },
    },
  ],
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1',
    cwd: '.',
    url: 'http://127.0.0.1:5173',
    reuseExistingServer: true,
    timeout: 30000,
  },
});
