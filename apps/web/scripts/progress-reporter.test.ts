import { afterEach, expect, it, vi } from "vitest";

import ProgressReporter from "../e2e/helpers/progress-reporter";

afterEach(() => vi.restoreAllMocks());

it("prints a start before completion and an end with status, retry and duration", () => {
  const log = vi.spyOn(console, "info").mockImplementation(() => {});
  const reporter = new ProgressReporter();
  const test = { titlePath: () => ["chromium", "example.spec.ts", "does work"] };
  reporter.onTestBegin(test, { retry: 1 });
  expect(log).toHaveBeenCalledTimes(1);
  expect(log).toHaveBeenLastCalledWith(
    expect.stringMatching(/^\[e2e\] START .*chromium > example.spec.ts > does work retry=1$/),
  );
  reporter.onTestEnd(test, { status: "timedOut", retry: 1, duration: 30_000 });
  expect(log).toHaveBeenLastCalledWith(
    expect.stringMatching(/^\[e2e\] END .*status=timedOut retry=1 duration_ms=30000$/),
  );
});
