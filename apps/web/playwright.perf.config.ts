import { defineConfig, devices } from "@playwright/test";

/*
 * The keydown-to-paint gate (e2e/speed.spec.ts) against a production build.
 * React's development build captures an owner stack for each element
 * (Error() plus console.createTask for the first 10,000 after a second's
 * pause), which adds 10 to 15 ms to an open that users never pay. Every
 * other spec stays on the dev server (playwright.config.ts): large-list's
 * coverage gate reads /src/ script URLs.
 */
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
    // The production build registers the PWA service worker, which would
    // answer requests the specs stub with page.route.
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
  },
});
