import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TestResult } from "@playwright/test/reporter";
import fs from "node:fs";

import PeopleTraceReporter from "../e2e/helpers/people-trace-reporter";

// This reporter reads the runner's private bridge-token file. Supply an inert
// fake state instead of reading or modifying any active browser profile.

const result: Pick<TestResult, "status" | "retry"> = { status: "passed", retry: 0 };

function neverResponds() {
  return vi.fn<typeof fetch>(
    (_input, init) =>
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener(
          "abort",
          () => reject(new DOMException("private-test-token", "AbortError")),
          { once: true },
        );
      }),
  );
}

describe("optional people observer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(fs, "readFileSync").mockReturnValue(
      JSON.stringify({ bridgeUrl: "http://127.0.0.1:19999", token: "private-test-token" }),
    );
    vi.spyOn(console, "info").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("allows at most one observation when Playwright does not await test-end hooks", async () => {
    const request = neverResponds();
    vi.stubGlobal("fetch", request);
    const reporter = new PeopleTraceReporter();
    const first = reporter.onTestEnd({ title: "first" }, result);
    const second = reporter.onTestEnd({ title: "second" }, result);
    expect(request).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(2_000);
    await Promise.all([first, second]);
  });

  it("aborts a never-responding fetch at the observer deadline and releases its slot", async () => {
    const request = neverResponds();
    vi.stubGlobal("fetch", request);
    const reporter = new PeopleTraceReporter();
    const pending = reporter.onTestEnd({ title: "stalled" }, result);
    await vi.advanceTimersByTimeAsync(1_999);
    expect(request.mock.calls[0]?.[1]?.signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await pending;
    expect(request.mock.calls[0]?.[1]?.signal?.aborted).toBe(true);
    expect(console.info).toHaveBeenCalledWith(expect.stringContaining("request=people"));
    expect(JSON.stringify(vi.mocked(console.info).mock.calls)).not.toContain("private-test-token");
    request.mockImplementation(async () => new Response("{}", { status: 200 }));
    await reporter.onTestEnd({ title: "recovered" }, result);
    expect(request.mock.calls.length).toBeGreaterThan(1);
  });

  it("aborts at suite end and never starts requests after shutdown", async () => {
    const request = neverResponds();
    vi.stubGlobal("fetch", request);
    const reporter = new PeopleTraceReporter();
    const pending = reporter.onTestEnd({ title: "last" }, result);
    reporter.onEnd();
    await pending;
    expect(request.mock.calls[0]?.[1]?.signal?.aborted).toBe(true);
    await reporter.onTestEnd({ title: "late hook" }, result);
    expect(request).toHaveBeenCalledTimes(1);
  });

  it("uses one total deadline across sequential diagnostic endpoints", async () => {
    const request = neverResponds();
    request.mockImplementationOnce(async () => new Response("{}", { status: 200 }));
    vi.stubGlobal("fetch", request);
    const reporter = new PeopleTraceReporter();
    const pending = reporter.onTestEnd({ title: "second endpoint stalls" }, result);
    await vi.advanceTimersByTimeAsync(1_999);
    expect(request).toHaveBeenCalledTimes(2);
    expect(request.mock.calls[1]?.[1]?.signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await pending;
    expect(request.mock.calls[1]?.[1]?.signal?.aborted).toBe(true);
    expect(console.info).toHaveBeenCalledWith(expect.stringContaining("request=person"));
  });

  it("reports an HTTP failure without logging its body or credentials", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>(
        async () => new Response("private-test-token private body", { status: 503 }),
      ),
    );
    const reporter = new PeopleTraceReporter();
    await reporter.onTestEnd({ title: "unavailable" }, result);
    expect(console.info).toHaveBeenCalledWith(expect.stringContaining("status=503"));
    expect(JSON.stringify(vi.mocked(console.info).mock.calls)).not.toContain("private-test-token");
    expect(JSON.stringify(vi.mocked(console.info).mock.calls)).not.toContain("private body");
  });
});
