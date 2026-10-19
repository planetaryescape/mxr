import { expect, type Page } from "@playwright/test";

import { deliver } from "./spool";
import { bridge } from "./state";

interface ReplyRow {
  id: string;
  topics: { thread_id: string; subject: string; state: string }[];
}

/** Each mutating spec owns its request, not a turn another spec may answer. */
export async function freshReplyRequest(page: Page): Promise<{ person: string; thread: string }> {
  const unique = `reply-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const person = `person:${unique}@team.example`;
  deliver({
    from: `Reply acceptance <${unique}@team.example>`,
    to: "Alex Demo <alex@demo.mxr.local>",
    subject: unique,
    body: "Hi Alex,\n\nAlex, could you confirm the meeting time?\n\nThanks,\nReply acceptance",
  });
  await bridge(page, "/api/v1/mail/sync", {});
  let thread: string | undefined;
  await expect
    .poll(
      async () => {
        const answer = await bridge<{ messages: { your_turn: ReplyRow[] } }>(
          page,
          "/api/v1/mail/people",
        );
        thread = answer.messages.your_turn
          .find((row) => row.id === person)
          ?.topics.find(
            (topic) => topic.subject === unique && topic.state === "your_turn",
          )?.thread_id;
        return thread;
      },
      { timeout: 15_000 },
    )
    .toBeTruthy();
  if (!thread) throw new Error("Fresh owned reply request did not become Your turn");
  return { person, thread };
}
