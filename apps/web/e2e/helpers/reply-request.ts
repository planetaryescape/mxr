import { expect, type Page } from "@playwright/test";

import { deliver } from "./spool";
import { bridge } from "./state";

interface ReplyRow {
  id: string;
  topics: { thread_id: string; subject: string; state: string }[];
}

interface AttentionRow {
  thread_id: string;
  subject: string;
  counterparty_email: string;
  message_id: string;
  message_ids: string[];
}

interface Desk {
  owed: { rows: AttentionRow[] };
  people_new: { rows: AttentionRow[] };
}

/** Each mutating spec owns its request, not a turn another spec may answer. */
export async function freshReplyRequest(
  page: Page,
): Promise<{ person: string; thread: string; messageIds: string[] }> {
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
  const desk = await bridge<Desk>(page, "/api/v1/mail/desk?lane_limit=5000");
  const row = [...desk.owed.rows, ...desk.people_new.rows].find(
    (entry) => entry.thread_id === thread,
  );
  if (!row) throw new Error("Fresh request is missing from the actual desk");
  return { person, thread, messageIds: row.message_ids };
}

/** Establish correspondence before each current ask: a new sender alone is not owed. */
export async function freshOwedConversations(
  page: Page,
  topicCounts: readonly number[],
): Promise<{ rows: AttentionRow[]; messageIds: string[] }> {
  const unique = `owed-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  const senders = topicCounts.map((count, index) => {
    const email = `${unique}-${index}@team.example`;
    const friend = `Reply friend ${index} ${unique} <${email}>`;
    for (let topic = 0; topic < count; topic += 1) {
      deliver({
        from: "Alex Demo <alex@demo.mxr.local>",
        to: friend,
        subject: `${unique}-${index}-${topic}`,
        body: "Hi,\nThanks for discussing the project.",
        labels: ["SENT"],
        thread: `${unique}-${index}-${topic}`,
      });
      deliver({
        from: friend,
        to: "Alex Demo <alex@demo.mxr.local>",
        subject: `${unique}-${index}-${topic}`,
        body: "Hi Alex,\n\nAlex, could you confirm the meeting time?\n\nThanks,\nYour colleague",
        thread: `${unique}-${index}-${topic}`,
      });
    }
    return { email, count };
  });
  const expected = topicCounts.reduce((sum, count) => sum + count, 0);
  await bridge(page, "/api/v1/mail/sync", {});
  let rows: AttentionRow[] = [];
  await expect
    .poll(
      async () => {
        const desk = await bridge<Desk>(page, "/api/v1/mail/desk?lane_limit=5000");
        rows = desk.owed.rows.filter((row) =>
          senders.some((sender) => sender.email === row.counterparty_email),
        );
        return rows.length;
      },
      { timeout: 15_000 },
    )
    .toBe(expected);
  await expect
    .poll(
      async () => {
        const answer = await bridge<{ messages: { your_turn: ReplyRow[] } }>(
          page,
          "/api/v1/mail/people",
        );
        return senders.every(
          ({ email, count }) =>
            (answer.messages.your_turn.find((row) => row.id === `person:${email}`)?.topics.length ??
              0) === count,
        );
      },
      { timeout: 15_000 },
    )
    .toBe(true);
  return { rows, messageIds: rows.flatMap((row) => row.message_ids) };
}
