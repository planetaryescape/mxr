import { describe, expect, test } from "vitest";

import { checkScript, roundAllowed } from "./check-shape.mjs";

const messages = (source, options) =>
  checkScript(source, options).map((finding) => finding.message);

describe("check-shape: round and large radii are caught", () => {
  test.each([
    ['<span className="rounded-full" />', "rounded-full"],
    ['<span className="md:rounded-full bg-muted" />', "rounded-full"],
    ['<span className="hover:rounded-t-full" />', "rounded-t-full"],
    ['<div className="rounded-lg border" />', "rounded-lg"],
    ['<div className="rounded-xl" />', "rounded-xl"],
    ['<div className="rounded-2xl" />', "rounded-2xl"],
    ['<div className="rounded-tl-xl" />', "rounded-tl-xl"],
    ['<div className="rounded-[8px]" />', "rounded-[8px]"],
    ['<div className="rounded-[0.5rem]" />', "rounded-[0.5rem]"],
    ['<div className="rounded-[3.5px]" />', "rounded-[3.5px]"],
    ['<div className={cn("x", on ? "rounded-full" : "")} />', "rounded-full"],
  ])("%s", (source, expected) => {
    expect(messages(source).join("\n")).toContain(expected);
  });

  test("reports the line of each finding, in order", () => {
    expect(
      checkScript('<a>\n<b className="rounded-xl" />\n<c className="rounded-full" />'),
    ).toEqual([
      { line: 2, message: "rounded-xl: surfaces are square and controls use rounded-md (3px)" },
      {
        line: 3,
        message: "rounded-full: round is for Avatar, UnreadDot, StatusDot and CountBadge",
      },
    ]);
  });
});

describe("check-shape: valid shapes pass", () => {
  test.each([
    '<div className="rounded-none rounded-sm rounded-md" />',
    '<div className="rounded-[3px] rounded-[2px] rounded-[0.125rem]" />',
    '<div className="rounded-[inherit]" />',
    '<div className="rounded-t-md rounded-bl-sm" />',
    '<div className="data-[x=y]:rounded-sm" />',
    '<div className="not-rounded-full-here rounded-fullish" />',
    '<div className="border-full rounded" />',
  ])("%s", (source) => {
    expect(checkScript(source)).toEqual([]);
  });

  test("comments may name the classes they forbid", () => {
    expect(
      checkScript(
        '// no rounded-full here\n/* rounded-xl\n   rounded-full */\n<a href="https://x.test/rounded-full" />',
      ).map((finding) => finding.line),
    ).toEqual([4]);
  });
});

describe("check-shape: the round components", () => {
  test("rounded-full passes only where the file is allowed it", () => {
    const source = '<span className="rounded-full" />';
    expect(checkScript(source, { round: true })).toEqual([]);
    expect(checkScript(source, { round: false })).toHaveLength(1);
  });

  test("allowing round does not allow large radii", () => {
    expect(checkScript('<span className="rounded-xl" />', { round: true })).toHaveLength(1);
  });

  test("the allowlist names whole files, not directories or prefixes", () => {
    expect(roundAllowed("components/Avatar.tsx")).toBe(true);
    expect(roundAllowed("components/AvatarStack.tsx")).toBe(false);
    expect(roundAllowed("features/messages/components/Avatar.tsx")).toBe(false);
  });
});
