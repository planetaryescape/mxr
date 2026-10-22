import { describe, expect, test } from "vitest";

import {
  FORWARD_HEADER,
  joinTextRegion,
  splitTextRegion,
  utf16Index,
  utf8Offset,
} from "./textRegion";

describe("splitTextRegion", () => {
  test("a reply's text ends before the signature block", () => {
    const body = "Thursday works.\n\n-- \nBK\nmxr";
    const region = splitTextRegion(body);
    expect(region.text).toBe("Thursday works.");
    expect(region.prefix).toBe("");
    expect(region.suffix).toBe("\n\n-- \nBK\nmxr");
  });

  test("the web's `--` delimiter counts as a signature too", () => {
    expect(splitTextRegion("Hi\n\n--\nBK").text).toBe("Hi");
  });

  test("a forward keeps its header in the prefix and its signature below", () => {
    const body = `${FORWARD_HEADER}\n\nPlease see below\n\n-- \nBK`;
    const region = splitTextRegion(body);
    expect(region.text).toBe("Please see below");
    expect(region.prefix).toBe(`${FORWARD_HEADER}\n\n`);
    expect(region.suffix).toBe("\n\n-- \nBK");
  });

  test("a forward whose signature sits above the header keeps it in the prefix", () => {
    const body = `-- \nBK\n\n${FORWARD_HEADER}\n\nFYI`;
    const region = splitTextRegion(body);
    expect(region.text).toBe("FYI");
    expect(region.prefix).toBe(`-- \nBK\n\n${FORWARD_HEADER}\n\n`);
    expect(region.suffix).toBe("");
  });

  test("a body with no delimiter is all text", () => {
    expect(splitTextRegion("  Just words  ").text).toBe("Just words");
  });

  test("an empty body and a signature-only body have empty text", () => {
    expect(splitTextRegion("").text).toBe("");
    expect(splitTextRegion("-- \nBK").text).toBe("");
  });
});

describe("joinTextRegion", () => {
  test("accepting writes the new text and keeps the signature", () => {
    const body = "Thursday works.\n\n-- \nBK";
    const region = splitTextRegion(body);
    const { body: next, textStart } = joinTextRegion(region, "Thursday works. Any time after 2.");
    expect(next).toBe("Thursday works. Any time after 2.\n\n-- \nBK");
    expect(next.slice(textStart, textStart + "Thursday works. Any time after 2.".length)).toBe(
      "Thursday works. Any time after 2.",
    );
  });

  test("a round trip through an unchanged region is exact for blank-line bodies", () => {
    for (const body of [
      "Hi Pat,\n\nThursday works.\n\n-- \nBK",
      `${FORWARD_HEADER}\n\nFYI\n\n-- \nBK`,
      "Only words",
    ]) {
      const region = splitTextRegion(body);
      expect(joinTextRegion(region, region.text).body).toBe(body);
    }
  });

  test("text lands after a forward header when the body was only the header", () => {
    const region = splitTextRegion(FORWARD_HEADER);
    const { body, textStart } = joinTextRegion(region, "Look at this");
    expect(body).toBe(`${FORWARD_HEADER}\n\nLook at this`);
    expect(textStart).toBe(FORWARD_HEADER.length + 2);
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
