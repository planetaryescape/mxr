import { describe, expect, test } from "vitest";

import { isLongThread } from "./longThread";

const message = (id: string) => ({ id }) as never;
const words = (count: number) => Array.from({ length: count }, (_, i) => `w${i}`).join(" ");

describe("isLongThread", () => {
  test("four messages is long whatever their length", () => {
    const messages = ["a", "b", "c", "d"].map(message);
    expect(isLongThread({ messages, bodies: [] })).toBe(true);
    expect(isLongThread({ messages: messages.slice(0, 3), bodies: [] })).toBe(false);
  });

  test("over 400 words of reader text is long, counted across messages", () => {
    const messages = ["a", "b"].map(message);
    const at = (count: number) => [
      { message_id: "a", reader_text: words(200) },
      { message_id: "b", reader_text: words(count) },
    ];
    expect(isLongThread({ messages, bodies: at(200) })).toBe(false);
    expect(isLongThread({ messages, bodies: at(201) })).toBe(true);
  });

  test("falls back to the plain part, then the HTML without its tags", () => {
    const messages = [message("a")];
    expect(isLongThread({ messages, bodies: [{ message_id: "a", text_plain: words(401) }] })).toBe(
      true,
    );
    const html = `<p class="${"x ".repeat(500)}">${words(10)}</p>`;
    expect(isLongThread({ messages, bodies: [{ message_id: "a", text_html: html }] })).toBe(false);
  });
});
