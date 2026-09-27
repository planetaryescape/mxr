/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import { normalizeSegments, splitMessageText } from "./textSegments";

const kinds = (text: string) =>
  normalizeSegments(splitMessageText(text)).map((segment) => segment.kind);

describe("splitMessageText", () => {
  test("a long run after -- is body text, not a signature", () => {
    const body = Array.from({ length: 12 }, (_, index) => `Point ${index + 1}.`);
    expect(kinds(["Notes:", "--", ...body].join("\n"))).toEqual(["text"]);
    expect(kinds(["Thanks,", "-- ", "Ada", "Engines Ltd"].join("\n"))).toEqual([
      "text",
      "signature",
    ]);
  });

  test("folds a top-posted reply under its Gmail attribution", () => {
    const text = [
      "Sounds good, ship it.",
      "",
      "On Mon, Sep 21, 2026 at 10:02 AM Ada Lovelace <ada@example.com> wrote:",
      "> Here is the plan.",
      "> Step one.",
    ].join("\n");
    const segments = normalizeSegments(splitMessageText(text));
    expect(segments.map((s) => s.kind)).toEqual(["text", "quote"]);
    expect(segments[0]?.text.trim()).toBe("Sounds good, ship it.");
    expect(segments[1]?.text).toMatch(/^On Mon/);
  });

  test("handles an attribution wrapped across two lines", () => {
    const text = [
      "Thanks!",
      "On Mon, Sep 21, 2026 at 10:02 AM Ada Lovelace",
      "<ada@example.com> wrote:",
      "> hi",
    ].join("\n");
    expect(kinds(text)).toEqual(["text", "quote"]);
  });

  test("treats an Outlook header block as the start of quoted mail", () => {
    const text = [
      "See below.",
      "",
      "From: Grace Hopper <grace@example.com>",
      "Sent: Monday, September 21, 2026 9:00 AM",
      "To: Team <team@example.com>",
      "Subject: Launch",
      "",
      "Original body",
    ].join("\n");
    expect(kinds(text)).toEqual(["text", "quote"]);
  });

  test("keeps a one- or two-line inline quote as text", () => {
    const text = ["> Can you send the file?", "Attached."].join("\n");
    expect(kinds(text)).toEqual(["text"]);
  });

  test("folds long interleaved quote runs but keeps replies between them", () => {
    const quote = ["> a", "> b", "> c", "> d"].join("\n");
    const text = [quote, "My answer to that.", quote].join("\n");
    expect(kinds(text)).toEqual(["quote", "text", "quote"]);
  });

  test("separates an RFC 3676 signature", () => {
    const text = ["Hello there.", "-- ", "Ada", "Analytical Engines Ltd"].join("\n");
    const segments = normalizeSegments(splitMessageText(text));
    expect(segments.map((s) => s.kind)).toEqual(["text", "signature"]);
    expect(segments[1]?.text).toBe("Ada\nAnalytical Engines Ltd");
  });

  test("ends a signature where quoted mail begins", () => {
    const text = ["Hi", "--", "Ada", "> quoted 1", "> quoted 2", "> quoted 3", "> quoted 4"].join(
      "\n",
    );
    expect(kinds(text)).toEqual(["text", "signature", "quote"]);
  });

  test("leaves a plain message untouched", () => {
    const text = "Just a note.\n\nWith two paragraphs.";
    const segments = splitMessageText(text);
    expect(segments).toHaveLength(1);
    expect(segments[0]).toMatchObject({ kind: "text", text });
  });
});
