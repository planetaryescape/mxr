import { describe, expect, test } from "vitest";

import { allowed, checkScript } from "./check-type-scale.mjs";

const messages = (source) => checkScript(source).map((finding) => finding.message);

describe("check-type-scale: literal sizes are caught", () => {
  test.each([
    ['<p className="text-[13px]" />', "text-[13px]"],
    ['<p className="md:text-[12.5px]" />', "text-[12.5px]"],
    ['<p className="hover:text-[10.5px] font-mono" />', "text-[10.5px]"],
    ['<p className="text-[1.1rem]" />', "text-[1.1rem]"],
    ['<p className={cn("x", lead ? "text-[17px]" : "")} />', "text-[17px]"],
  ])("%s", (source, expected) => {
    expect(messages(source).join("\n")).toContain(expected);
  });

  test("reports the line of each literal", () => {
    expect(
      checkScript('<a>\n<b className="text-[14px]" />\n<c className="text-[11px]" />'),
    ).toEqual([
      { line: 2, message: "text-[14px]: use a text-* token from tokens.css" },
      { line: 3, message: "text-[11px]: use a text-* token from tokens.css" },
    ]);
  });
});

describe("check-type-scale: valid classes pass", () => {
  test.each([
    '<p className="text-[length:var(--mail-row-subject-size)]" />',
    '<p className="text-[var(--font-size-x)]" />',
    '<p className="text-2xs text-xs text-sm text-base text-md text-lg text-xl" />',
    '<p className="grid-cols-[minmax(0,1fr)] max-w-[72rem] tracking-[0.12em]" />',
    '<p className="[&_.text-foo]:text-muted-foreground" />',
    '<div style={{ fontSize: "13px" }} />',
  ])("%s", (source) => {
    expect(checkScript(source)).toEqual([]);
  });
});

describe("check-type-scale: allowlist", () => {
  test("a file keeps its literal only where it is listed", () => {
    expect(allowed("features/reading/ReadingReader.tsx")).toBe(true);
    expect(allowed("features/reading/ReadingRoute.tsx")).toBe(false);
  });

  test("a directory entry covers files below it, not files that share a prefix", () => {
    const directory = new Map([["features/updates", "a folder under migration"]]);
    expect(allowed("features/updates/UpdateLineRow.tsx", directory)).toBe(true);
    expect(allowed("features/updates-old/Thing.tsx", directory)).toBe(false);
  });
});
