import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import type { Reporter, TestCase, TestResult } from "@playwright/test/reporter";

/**
 * Specs share one daemon, so a spec that leaves a person somewhere else
 * breaks a later spec far from the cause. After every test this prints one
 * line whenever Noor's band (the person verbs' mode-done needs in Your turn)
 * or the standing corrections change, naming the test that just ran.
 */
const WATCHED = "person:noor@tidewater.example";

interface State {
  bridgeUrl: string;
  token: string;
}

interface Row {
  id?: unknown;
  band?: unknown;
  topics?: { state?: unknown }[];
}

interface Observation {
  messages?: Record<string, Row[] | number>;
  page?: { topics?: { thread_id?: string }[] };
  threads?: { modes?: { mode?: string }[] }[];
  corrections?: { id: number; undone_at?: unknown; to_mode: string; scope: string }[];
}

export default class PeopleTraceReporter implements Reporter {
  private last = "";
  private thread: string | null = null;
  private active: AbortController | null = null;
  private closed = false;

  async onTestEnd(
    test: Pick<TestCase, "title">,
    result: Pick<TestResult, "status" | "retry">,
  ): Promise<void> {
    // Playwright does not await onTestEnd. Diagnostics must not accumulate
    // concurrent daemon reads behind the suite's next tests.
    if (this.active || this.closed) return;
    const controller = new AbortController();
    this.active = controller;
    const started = Date.now();
    const deadline = setTimeout(() => controller.abort(), 2_000);
    let request = "state";
    let status: number | undefined;
    try {
      // SAFETY: e2e-server.mjs writes this private fake-profile state file.
      // Malformed diagnostic input is caught and never controls a test.
      const state = JSON.parse(readFileSync(resolve(".playwright/state.json"), "utf8")) as State;
      const get = async (name: string, path: string): Promise<Observation> => {
        request = name;
        status = undefined;
        const response = await fetch(`${state.bridgeUrl}${path}`, {
          headers: { authorization: `Bearer ${state.token}` },
          signal: controller.signal,
        });
        status = response.status;
        if (!response.ok) throw new Error("Observer HTTP failure");
        return response.json();
      };
      const people = await get("people", "/api/v1/mail/people");
      let noor = "ABSENT";
      for (const [band, rows] of Object.entries(people.messages ?? {})) {
        if (!Array.isArray(rows)) continue;
        for (const row of rows) {
          if (row.id === WATCHED) {
            const topics = (row.topics ?? []).map((topic) => String(topic.state)).join(",");
            noor = `${band} (topics: ${topics})`;
          }
        }
      }
      // Noor's one conversation: found once, then asked where it is held.
      if (!this.thread) {
        const page = await get(
          "person",
          `/api/v1/mail/people/page?person=${encodeURIComponent(WATCHED)}`,
        );
        this.thread = page.page?.topics?.[0]?.thread_id ?? null;
      }
      let held = "unknown thread";
      if (this.thread) {
        const membership = await get(
          "membership",
          `/api/v1/mail/modes/membership?thread_id=${this.thread}`,
        );
        held = (membership.threads?.[0]?.modes ?? []).map((entry) => entry.mode).join(" | ");
      }
      const corrections = await get("corrections", "/api/v1/mail/corrections?limit=200");
      const standing = (corrections.corrections ?? [])
        .filter((c) => !c.undone_at)
        .map((c) => `${c.id}:${c.scope}->${c.to_mode}`)
        .join(" ");
      const now = `Noor ${noor}; held in [${held}]; standing corrections [${standing}]`;
      if (now !== this.last) {
        this.last = now;
        console.info(
          `[people-trace] after "${test.title}" (${result.status}, retry ${result.retry}): ${now}`,
        );
      }
    } catch (error) {
      const kind = controller.signal.aborted
        ? "aborted"
        : error instanceof Error
          ? error.name
          : "unknown";
      console.info(
        `[people-trace] unavailable after "${test.title}" request=${request} status=${status ?? "pending"} elapsed_ms=${Date.now() - started} error=${kind}`,
      );
    } finally {
      clearTimeout(deadline);
      this.active = null;
    }
  }

  onEnd(): void {
    this.closed = true;
    this.active?.abort();
  }
}
