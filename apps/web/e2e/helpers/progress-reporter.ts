import type { Reporter, TestCase, TestResult } from "@playwright/test/reporter";

/** CI's GitHub annotations do not show passing progress or a currently stuck test. */
export default class ProgressReporter implements Reporter {
  onTestBegin(test: Pick<TestCase, "titlePath">, result: Pick<TestResult, "retry">): void {
    console.info(
      `[e2e] START ${new Date().toISOString()} ${test.titlePath().join(" > ")} retry=${result.retry}`,
    );
  }

  onTestEnd(
    test: Pick<TestCase, "titlePath">,
    result: Pick<TestResult, "status" | "retry" | "duration">,
  ): void {
    console.info(
      `[e2e] END ${new Date().toISOString()} ${test.titlePath().join(" > ")} status=${result.status} retry=${result.retry} duration_ms=${result.duration}`,
    );
  }
}
