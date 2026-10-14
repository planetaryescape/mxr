import { describe, expect, test } from "vitest";

import { PREVIEW_LINES, previewLines, previewSource } from "./emailPreview";

describe("previewLines", () => {
  test("takes the first informative lines, skipping greetings, links and footers", () => {
    const text = [
      "",
      "Hi Bhekani,",
      "[image: Acme logo]",
      "https://acme.example/track/abc",
      "Your invoice INV-204 for $1,240.00 is due on 20 Oct.",
      "> Pay by card or bank transfer.",
      "Total   $1,240.00",
      "Paid",
      "Unsubscribe or manage your preferences",
      "Last line that would be fifth.",
      "And a sixth.",
    ].join("\n");
    expect(previewLines(text)).toEqual([
      "Your invoice INV-204 for $1,240.00 is due on 20 Oct.",
      "Pay by card or bank transfer.",
      "Total $1,240.00",
      "Last line that would be fifth.",
    ]);
    expect(previewLines(text)).toHaveLength(PREVIEW_LINES);
  });

  test("keeps a short line that carries a figure and drops a lone label", () => {
    expect(previewLines("Total\n$24.00\nBalance 3")).toEqual(["$24.00", "Balance 3"]);
  });

  test("a long line is cut at a word with an ellipsis, and repeats collapse", () => {
    const long = `${"word ".repeat(60)}end`;
    const [line] = previewLines(long);
    expect(line?.length).toBeLessThanOrEqual(160);
    expect(line?.endsWith("…")).toBe(true);
    expect(previewLines("Same line here\nSame line here\nOther line here")).toEqual([
      "Same line here",
      "Other line here",
    ]);
  });

  test("markup stays text, and a missing or odd-typed body gives nothing", () => {
    expect(previewLines("<script>alert(1)</script> and more text")).toEqual([
      "<script>alert(1)</script> and more text",
    ]);
    for (const value of [undefined, null, "", "   \n\n", 5, {}, ["a b"]]) {
      expect(previewLines(value as never)).toEqual([]);
    }
  });

  test("only the first lines of a long body are read", () => {
    const body = `${"\n".repeat(200)}A real line of text`;
    expect(previewLines(body)).toEqual([]);
  });
});

describe("previewSource", () => {
  test("prefers the reader's cleaned text, then the plain part", () => {
    expect(previewSource({ message_id: "m", reader_text: "clean", text_plain: "plain" })).toBe(
      "clean",
    );
    expect(previewSource({ message_id: "m", reader_text: "", text_plain: "plain" })).toBe("plain");
    expect(previewSource({ message_id: "m" })).toBeNull();
    expect(previewSource(undefined)).toBeNull();
  });
});
