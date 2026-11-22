import { defineConfig, devices } from "@playwright/test";

const appUrl = `http://127.0.0.1:${process.env.MXR_E2E_APP_PORT ?? "5173"}`;

export default defineConfig({
  testDir: "./e2e",
  testMatch: ["**/speed.spec.ts"],
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: appUrl,
    trace: "on-first-retry",
    serviceWorkers: "block",
    launchOptions: process.env.PW_CHROME ? { executablePath: process.env.PW_CHROME } : {},
  },
  projects: [{ name: "chromium-prod", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "node ./scripts/e2e-server.mjs",
    env: { MXR_E2E_APP_MODE: "preview" },
    url: appUrl,
    reuseExistingServer: false,
    timeout: 180_000,
    gracefulShutdown: { signal: "SIGTERM", timeout: 10_000 },
  },
});
