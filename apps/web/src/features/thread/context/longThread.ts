/*
 * When the reader shows the gist. The list rows already said what a
 * conversation is about; once it is open, a one-line gist only saves
 * reading when there is a lot to read. Four or more messages is where the
 * thread of who asked what gets lost, and 400 words is about two minutes
 * of reading at a skim, more than one screen of the reader. Below both,
 * the messages are the gist.
 */

import type { ThreadResponse } from "@/features/mailbox/types";

export const LONG_THREAD_MESSAGES = 4;
export const LONG_THREAD_WORDS = 400;

/** Words the reader would show: the cleaned text, not quoted history. */
function bodyWords(body: ThreadResponse["bodies"][number]): number {
  const text =
    body.reader_text ?? body.text_plain ?? body.text_html?.replace(/<[^>]*>/g, " ") ?? "";
  return text.split(/\s+/).filter(Boolean).length;
}

export function isLongThread(data: Pick<ThreadResponse, "messages" | "bodies">): boolean {
  if (data.messages.length >= LONG_THREAD_MESSAGES) return true;
  let words = 0;
  for (const body of data.bodies) {
    words += bodyWords(body);
    if (words > LONG_THREAD_WORDS) return true;
  }
  return false;
}
