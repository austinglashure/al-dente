import { defineConfig, devices } from '@playwright/test';

// End-to-end tests start the real `aldente serve` on fixture copies
// (see e2e/server.ts). Build the binary first: `npm run e2e` does.
export default defineConfig({
  testDir: 'e2e',
  timeout: 60_000,
  retries: 0,
  use: { trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], channel: 'chromium' } }],
});
