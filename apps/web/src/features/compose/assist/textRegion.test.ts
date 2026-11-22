import { describe, expect, test } from "vitest";

import { FORWARD_HEADER, joinTextRegion, splitTextRegion } from "./textRegion";

describe("splitTextRegion mirrors the daemon's split_body", () => {
  test.each([
    {
      name: "a signature ends the text",
      body: "Thursday works.\n\n-- \nAlex\nFoundry",
      prefix: "",
      text: "Thursday works.",
      suffix: "\n\n-- \nAlex\nFoundry",
    },
    {
      name: "a forward with a signature ends the text at the signature",
      body: `Fyi.\n\n-- \nAlex\n\n${FORWARD_HEADER}\nFrom: a`,
      prefix: "",
      text: "Fyi.",
      suffix: `\n\n-- \nAlex\n\n${FORWARD_HEADER}\nFrom: a`,
    },
    {
      name: "a forward without a signature ends the text at the header",
      body: `Fyi.\n\n${FORWARD_HEADER}`,
      prefix: "",
      text: "Fyi.",
      suffix: `\n\n${FORWARD_HEADER}`,
    },
    {
      name: "a body that starts with its signature has no text",
      body: "-- \nAlex",
      prefix: "",
      text: "",
      suffix: "-- \nAlex",
    },
    {
      name: "a body that starts with the forward header has no text",
      body: `${FORWARD_HEADER}\nFrom: a`,
      prefix: "",
      text: "",
      suffix: `${FORWARD_HEADER}\nFrom: a`,
    },
    {
      name: "a dash inside a line is not a signature",
      body: "Two -- no, three things.\n--- not a rule",
      prefix: "",
      text: "Two -- no, three things.\n--- not a rule",
      suffix: "",
    },
    {
      name: "CRLF bodies cut at the signature",
      body: "Hi\r\n\r\n-- \r\nBK",
      prefix: "",
      text: "Hi",
      suffix: "\r\n\r\n-- \r\nBK",
    },
    {
      name: "leading blank lines stay in the prefix, outside the text",
      body: "\n\nHi\n\n-- \nBK",
      prefix: "\n\n",
      text: "Hi",
      suffix: "\n\n-- \nBK",
    },
  ])("$name", ({ body, prefix, text, suffix }) => {
    const region = splitTextRegion(body);
    expect({ prefix: region.prefix, text: region.text, suffix: region.suffix }).toEqual({
      prefix,
      text,
      suffix,
    });
    expect(region.prefix + region.text + region.suffix).toBe(body);
  });
});

describe("joinTextRegion", () => {
  test("a round trip through an unchanged region returns the body exactly", () => {
    for (const body of [
      "Hi Pat,\n\nThursday works.\n\n-- \nBK",
      `FYI\n\n-- \nsig\n\n${FORWARD_HEADER}\nOriginal`,
      "Hi\r\n\r\n-- \r\nBK",
      "Hi\n-- \nBK",
      "Only words",
    ]) {
      const region = splitTextRegion(body);
      expect(joinTextRegion(region, region.text).body).toBe(body);
    }
  });
});
