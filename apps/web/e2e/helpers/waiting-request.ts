import { expect, type Page } from "@playwright/test";

import { deliver } from "./spool";
import { bridge } from "./state";

interface WaitingRow {
  thread_id: string;
  message_id: string;
  message_ids: string[];
  subject: string;
  age_seconds: number;
}

interface WaitingFixture {
  rows: WaitingRow[];
  finish: () => Promise<void>;
}

async function waitingRows(page: Page): Promise<WaitingRow[]> {
  const desk = await bridge<{ waiting: { rows: WaitingRow[] } }>(
    page,
    "/api/v1/mail/desk?lane_limit=100",
  );
  return desk.waiting.rows;
}

/** Current outgoing asks old enough to wait on, paired with mail that asks nothing. */
export async function freshWaitingConversations(page: Page, count = 2): Promise<WaitingFixture> {
  const unique = `waiting-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const date = new Date(Date.now() - 48 * 3_600_000).toISOString();
  const subjects = Array.from({ length: count }, (_, index) => `${unique}-${index}`);
  for (const subject of [...subjects, `${unique}-no-ask`]) {
    deliver({
      from: "Alex Demo <alex@demo.mxr.local>",
      to: "Waiting colleague <waiting-colleague@team.example>",
      subject,
      thread: subject,
      labels: ["SENT"],
      date,
      body: subject.endsWith("-no-ask")
        ? "Hi,\n\nThanks for the meeting.\n\nAlex"
        : "Hi,\n\nCould you confirm the meeting time?\n\nThanks,\nAlex",
    });
  }
  await bridge(page, "/api/v1/mail/sync", {});
  let rows: WaitingRow[] = [];
  await expect
    .poll(
      async () => {
        const waiting = await waitingRows(page);
        expect(waiting.some((row) => row.subject === `${unique}-no-ask`)).toBe(false);
        rows = waiting.filter((row) => subjects.includes(row.subject));
        return rows.length;
      },
      { timeout: 15_000 },
    )
    .toBe(count);
  for (const row of rows) expect(row.age_seconds).toBeGreaterThanOrEqual(47 * 3_600);
  return {
    rows,
    finish: async () => {
      for (const subject of subjects) {
        deliver({
          from: "Waiting colleague <waiting-colleague@team.example>",
          to: "Alex Demo <alex@demo.mxr.local>",
          subject,
          thread: subject,
          body: "The meeting time is confirmed. Thanks, Alex.",
        });
      }
      await bridge(page, "/api/v1/mail/sync", {});
      await expect
        .poll(
          async () =>
            (await waitingRows(page)).filter((row) => subjects.includes(row.subject)).length,
          { timeout: 15_000 },
        )
        .toBe(0);
    },
  };
}
