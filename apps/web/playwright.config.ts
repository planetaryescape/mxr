import { defineConfig, devices } from "@playwright/test";

// Keep in sync with scripts/e2e-server.mjs, which reads the same variable.
const appUrl = `http://127.0.0.1:${process.env.MXR_E2E_APP_PORT ?? "5173"}`;

export default defineConfig({
  testDir: "./e2e",
  // The speed gate runs against a production build: playwright.perf.config.ts.
  testIgnore: ["**/speed.spec.ts"],
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI
    ? [
        ["github"],
        ["line"],
        ["./e2e/helpers/progress-reporter.ts"],
        ["./e2e/helpers/people-trace-reporter.ts"],
      ]
    : "list",
  use: {
    baseURL: appUrl,
    // The theme follows the OS until one is picked; the suite was written
    // against the dark default, so the browser reports a dark scheme.
    // MXR_E2E_SCHEME=light runs the whole suite in the light theme.
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
    // e2e-server runs the daemon and Vite in their own process groups, which
    // Playwright's default group SIGKILL never reaches; SIGTERM lets it stop them.
    gracefulShutdown: { signal: "SIGTERM", timeout: 10_000 },
  },
});
