import { defineConfig, devices } from "@playwright/test";

const appUrl = `http://127.0.0.1:${process.env.MXR_E2E_APP_PORT ?? "5173"}`;

export default defineConfig({
  testDir: "./e2e",
  testIgnore: ["**/speed.spec.ts"],
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI
    ? [["github"], ["line"], ["./e2e/helpers/progress-reporter.ts"]]
    : "list",
  use: {
    baseURL: appUrl,
    colorScheme: process.env.MXR_E2E_SCHEME === "light" ? "light" : "dark",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // Local escape hatch when the pinned Playwright browser isn't downloaded.
        // Chromium-only: a Chrome binary can't launch the WebKit project.
        launchOptions: process.env.PW_CHROME ? { executablePath: process.env.PW_CHROME } : {},
      },
    },
    // Safari's engine, which Tauri's WKWebView on macOS renders with. Opt-in:
    // Playwright runs every listed project when --project is absent, so the
    // project only exists when `npm run e2e:webkit` sets the variable and
    // `npm run e2e` stays Chromium-only.
    ...(process.env.MXR_E2E_WEBKIT === "1"
      ? [{ name: "webkit", use: { ...devices["Desktop Safari"] } }]
      : []),
  ],
  webServer: {
    command: "node ./scripts/e2e-server.mjs",
    url: appUrl,
    reuseExistingServer: false,
    timeout: 120_000,
    gracefulShutdown: { signal: "SIGTERM", timeout: 10_000 },
  },
});
