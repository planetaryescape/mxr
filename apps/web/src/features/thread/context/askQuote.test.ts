import { describe, expect, it } from "vitest";

import { ASK_MARK_ATTRIBUTE, findQuote, markQuoteInDocument } from "./askQuote";

const QUOTE = "Can you confirm who owns the rollout check before Monday?";

describe("findQuote", () => {
  it("finds the quote across reflowed line breaks", () => {
    const text =
      "Rollout risk.\nCan you confirm who owns the\nrollout check before Monday?\nThanks";
    const range = findQuote(text, QUOTE);
    expect(range).not.toBeNull();
    expect(text.slice(range!.start, range!.end)).toBe(
      "Can you confirm who owns the\nrollout check before Monday?",
    );
  });

  it("treats straight and curly apostrophes alike and escapes regex characters", () => {
    expect(findQuote("We’re at 5% (canary)? yes", "We're at 5% (canary)?")).toEqual({
      start: 0,
      end: 21,
    });
  });

  it("never matches a paraphrase or a different case", () => {
    expect(
      findQuote(QUOTE, "Could you confirm who owns the rollout check before Monday?"),
    ).toBeNull();
    expect(findQuote(QUOTE, QUOTE.toLowerCase())).toBeNull();
    expect(findQuote(QUOTE, "   ")).toBeNull();
  });
});

describe("markQuoteInDocument", () => {
  it("marks a quote split across elements and moves the mark on a second call", () => {
    const root = document.createElement("div");
    root.innerHTML =
      "<p>Rollout risk: watch latency.</p><p>Can you <b>confirm</b> who owns the rollout check before Monday?</p>";
    const first = markQuoteInDocument(root, QUOTE);
    expect(first?.tagName).toBe("MARK");
    const marks = root.querySelectorAll(`mark[${ASK_MARK_ATTRIBUTE}]`);
    expect(Array.from(marks, (mark) => mark.textContent).join("")).toBe(QUOTE);
    // The markup around the quote survives.
    expect(root.querySelector("b")?.textContent).toBe("confirm");

    expect(markQuoteInDocument(root, "Rollout risk: watch latency.")).not.toBeNull();
    expect(root.querySelectorAll(`mark[${ASK_MARK_ATTRIBUTE}]`)).toHaveLength(1);
    expect(root.textContent).toContain(QUOTE);
  });

  it("leaves the document alone when the quote isn't there", () => {
    const root = document.createElement("div");
    root.innerHTML = "<p>Nothing to see.</p>";
    expect(markQuoteInDocument(root, QUOTE)).toBeNull();
    expect(root.innerHTML).toBe("<p>Nothing to see.</p>");
  });
});
