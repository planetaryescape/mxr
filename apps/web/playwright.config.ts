import { defineConfig, devices } from "@playwright/test";

// Keep in sync with scripts/e2e-server.mjs, which reads the same variable.
const appUrl = `http://127.0.0.1:${process.env.MXR_E2E_APP_PORT ?? "5173"}`;

export default defineConfig({
  testDir: "./e2e",
  // The speed gate runs against a production build: playwright.perf.config.ts.
  testIgnore: ["**/speed.spec.ts"],
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: appUrl,
    // The theme follows the OS until one is picked; the suite was written
    // against the dark default, so the browser reports a dark scheme.
    colorScheme: "dark",
    trace: "on-first-retry",
    // Local escape hatch when the pinned Playwright browser isn't downloaded.
    launchOptions: process.env.PW_CHROME ? { executablePath: process.env.PW_CHROME } : {},
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: {
    command: "node ./scripts/e2e-server.mjs",
    url: appUrl,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
