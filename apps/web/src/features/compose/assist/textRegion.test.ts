import { describe, expect, test } from "vitest";

import {
  FORWARD_HEADER,
  joinTextRegion,
  splitTextRegion,
  utf16Index,
  utf8Offset,
} from "./textRegion";

/*
 * Mirrors the cases in crates/composer/src/check/regions.rs (`split_body`),
 * so the web and the daemon cut a body in the same place. The web keeps the
 * blank space after the text in the suffix, where the daemon keeps it in its
 * gap; both give the same text and the same bytes after it.
 */
describe("splitTextRegion mirrors the daemon's split_body", () => {
  test.each([
    {
      name: "a signature ends the text",
      body: "Thursday works.\n\n-- \nAlex\nFoundry",
      text: "Thursday works.",
      suffix: "\n\n-- \nAlex\nFoundry",
    },
    {
      name: "a forward with a signature ends the text at the signature",
      body: `Fyi.\n\n-- \nAlex\n\n${FORWARD_HEADER}\nFrom: a`,
      text: "Fyi.",
    },
    {
      name: "a forward without a signature ends the text at the header",
      body: `Fyi.\n\n${FORWARD_HEADER}`,
      text: "Fyi.",
      suffix: `\n\n${FORWARD_HEADER}`,
    },
    {
      name: "the forwarded original is never the text",
      body: `FYI\n\n${FORWARD_HEADER}\nOriginal words`,
      text: "FYI",
    },
    {
      name: "a forward with a signature keeps the original out of the text too",
      body: `FYI\n\n-- \nsig\n\n${FORWARD_HEADER}\nOriginal`,
      text: "FYI",
    },
    {
      name: "a body that starts with its signature has no text",
      body: "-- \nAlex",
      text: "",
      suffix: "-- \nAlex",
    },
    {
      name: "a body that starts with the forward header has no text",
      body: `${FORWARD_HEADER}\nFrom: a`,
      text: "",
      suffix: `${FORWARD_HEADER}\nFrom: a`,
    },
    {
      name: "a body with neither is all text",
      body: "Just this.\n",
      text: "Just this.",
      suffix: "\n",
    },
    {
      name: "a dash inside a line is not a signature",
      body: "Two -- no, three things.\n--- not a rule",
      text: "Two -- no, three things.\n--- not a rule",
    },
    {
      name: "CRLF bodies cut at the signature",
      body: "Hi\r\n\r\n-- \r\nBK",
      text: "Hi",
      suffix: "\r\n\r\n-- \r\nBK",
    },
    {
      name: "a delimiter with trailing spaces still cuts",
      body: "Hi\n--  \nBK",
      text: "Hi",
      suffix: "\n--  \nBK",
    },
    {
      name: "non-ASCII text before the cut keeps its characters",
      body: "Café — ok 🙂\n\n-- \nBK",
      text: "Café — ok 🙂",
      suffix: "\n\n-- \nBK",
    },
    {
      name: "leading blank lines stay in the prefix, outside the text",
      body: "\n\nHi\n\n-- \nBK",
      text: "Hi",
      prefix: "\n\n",
      suffix: "\n\n-- \nBK",
    },
  ])("$name", ({ body, text, suffix, prefix }) => {
    const region = splitTextRegion(body);
    expect(region.text).toBe(text);
    if (suffix !== undefined) expect(region.suffix).toBe(suffix);
    if (prefix !== undefined) expect(region.prefix).toBe(prefix);
    expect(region.prefix + region.text + region.suffix).toBe(body);
  });
});

describe("joinTextRegion", () => {
  test("replacing the text keeps the signature and the spacing (replacing_the_text_keeps_the_signature_and_the_spacing)", () => {
    const region = splitTextRegion("Old.\n-- \nAlex");
    expect(joinTextRegion(region, "New one.\n").body).toBe("New one.\n-- \nAlex");
  });

  test("a body that starts with its signature gets a break before the new text", () => {
    const { body, textStart } = joinTextRegion(splitTextRegion("-- \nAlex"), "Hi.");
    expect(body).toBe("Hi.\n\n-- \nAlex");
    expect(textStart).toBe(0);
  });

  test("a body with neither is replaced as it was written", () => {
    expect(joinTextRegion(splitTextRegion("Just this.\n"), "New.").body).toBe("New.\n");
  });

  test("the forward header stays below the new text", () => {
    const region = splitTextRegion(`FYI\n\n${FORWARD_HEADER}\nOriginal`);
    const { body, textStart } = joinTextRegion(region, "FYI, see below");
    expect(body).toBe(`FYI, see below\n\n${FORWARD_HEADER}\nOriginal`);
    expect(textStart).toBe(0);
  });

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

describe("accepting keeps the user's spacing byte for byte", () => {
  const accept = (body: string, proposal: string) =>
    joinTextRegion(splitTextRegion(body), proposal);

  test("a single newline before the signature is not widened to a blank line", () => {
    expect(accept("Hi\n-- \nBK", "Hi there").body).toBe("Hi there\n-- \nBK");
  });

  test("the signature and the spacing after the text come back exactly", () => {
    const body = "Thursday works.  \n\n\n-- \nBK\n";
    expect(accept(body, "Thursday works. Friday too.").body).toBe(
      "Thursday works. Friday too.  \n\n\n-- \nBK\n",
    );
  });

  test("a leading space in the proof is kept verbatim, so marks stay on the words", () => {
    const { body, textStart } = accept("Hi\n\n-- \nBK", " Hi there");
    expect(body).toBe(" Hi there\n\n-- \nBK");
    expect(body.slice(textStart, textStart + " Hi there".length)).toBe(" Hi there");
  });

  test("a signature-only body gets a break before the new text", () => {
    const { body, textStart } = accept("-- \nBK", "Hi");
    expect(body).toBe("Hi\n\n-- \nBK");
    expect(textStart).toBe(0);
  });

  test("an empty proof leaves the prefix and signature joined by one break", () => {
    expect(joinTextRegion({ prefix: "Hi", text: "", suffix: "-- \nBK" }, "").body).toBe(
      "Hi\n\n-- \nBK",
    );
  });
});

describe("UTF-8 offsets", () => {
  test("a selection after a multi-byte character is counted in bytes", () => {
    const text = "café au lait";
    // "é" is two bytes in UTF-8 but one UTF-16 unit.
    expect(utf8Offset(text, text.indexOf("au"))).toBe(6);
    expect(utf16Index(text, 6)).toBe(text.indexOf("au"));
  });

  test("emoji (surrogate pairs) round trip", () => {
    const text = "hi 🙂 there";
    const index = text.indexOf("there");
    expect(utf16Index(text, utf8Offset(text, index))).toBe(index);
  });
});
