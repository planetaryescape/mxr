import type { MessageBodyView } from "@/features/mailbox/types";

export const LONG_THREAD_MESSAGES = 4;
export const LONG_THREAD_WORDS = 400;

function bodyWords(body: MessageBodyView): number {
  const text =
    body.reader_text ?? body.text_plain ?? body.text_html?.replace(/<[^>]*>/g, " ") ?? "";
  return text.split(/\s+/).filter(Boolean).length;
}

export function isLongThread(data: {
  messages: readonly unknown[];
  bodies: readonly MessageBodyView[];
}): boolean {
  if (data.messages.length >= LONG_THREAD_MESSAGES) return true;
  let words = 0;
  for (const body of data.bodies) {
    words += bodyWords(body);
    if (words > LONG_THREAD_WORDS) return true;
  }
  return false;
}
