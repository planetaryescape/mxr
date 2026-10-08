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

export default class PeopleTraceReporter implements Reporter {
  private last = "";
  private thread: string | null = null;

  async onTestEnd(test: TestCase, result: TestResult): Promise<void> {
    try {
      const state = JSON.parse(
        readFileSync(resolve(".playwright/state.json"), "utf8"),
      ) as State;
      const get = async (path: string): Promise<unknown> => {
        const response = await fetch(`${state.bridgeUrl}${path}`, {
          headers: { authorization: `Bearer ${state.token}` },
        });
        return response.json();
      };
      const people = (await get("/api/v1/mail/people")) as {
        messages?: Record<string, Row[] | number>;
      };
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
        const page = (await get(
          `/api/v1/mail/people/page?person=${encodeURIComponent(WATCHED)}`,
        )) as { page?: { topics?: { thread_id?: string }[] } };
        this.thread = page.page?.topics?.[0]?.thread_id ?? null;
      }
      let held = "unknown thread";
      if (this.thread) {
        const membership = (await get(
          `/api/v1/mail/modes/membership?thread_id=${this.thread}`,
        )) as { threads?: { modes?: { mode?: string; reason?: string }[] }[] };
        held = (membership.threads?.[0]?.modes ?? [])
          .map((entry) => `${entry.mode}: ${entry.reason}`)
          .join(" | ");
      }
      const corrections = (await get("/api/v1/mail/corrections?limit=200")) as {
        corrections?: { id: number; undone_at?: unknown; to_mode: string; scope: string }[];
      };
      const standing = (corrections.corrections ?? [])
        .filter((c) => !c.undone_at)
        .map((c) => `${c.id}:${c.scope}->${c.to_mode}`)
        .join(" ");
      const now = `Noor ${noor}; held in [${held}]; standing corrections [${standing}]`;
      if (now !== this.last) {
        this.last = now;
        console.log(
          `[people-trace] after "${test.title}" (${result.status}, retry ${result.retry}): ${now}`,
        );
      }
    } catch (error) {
      console.log(`[people-trace] unavailable after "${test.title}": ${String(error)}`);
    }
  }
}
