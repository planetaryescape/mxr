import { describe, expect, test } from "vitest";

import { checkScript, checkStyles } from "./check-motion.mjs";

const messages = (findings) => findings.map((finding) => finding.message);

describe("check-motion: each way to sneak in motion is caught", () => {
  test.each([
    ['<div className="transition-colors" />', "transition-colors"],
    ['<div className="hover:transition-all" />', "hover:transition-all"],
    ['<div className="transition hover:bg-muted" />', "transition:"],
    ['<div className="transition-[width]" />', "transition-[width]"],
    ['<div className="duration-200" />', "duration-200"],
    ['<div className="duration-[350ms]" />', "duration-[350ms]"],
    ['<div className="ease-in" />', "ease-in"],
    ['<div className="[transition:color_200ms]" />', "transition: 200ms"],
    ['<div className="[transition:background-color_var(--x)]" />', "transition: only"],
    ['<div className="[animation:spin_1s_linear_infinite]" />', "animation: 1s"],
    ['<div className="[animation-duration:300ms]" />', "animation-duration: 300ms"],
    ['<div style={{ transition: "opacity 200ms ease-out" }} />', "transition: 200ms"],
    ['<div style={{ transition: "color var(--motion-duration-fast)" }} />', "transition: only"],
    ["<div style={{ animationDuration: '1.5s' }} />", "animation-duration: 1.5s"],
    ["<div style={{ transitionDuration: 200 }} />", "transition-duration: 200"],
    ["<div style={{ transitionProperty: `width` }} />", "transition-property: only"],
  ])("%s", (source, expected) => {
    expect(messages(checkScript(source)).join("\n")).toContain(expected);
  });

  test("CSS: literal times and colour transitions", () => {
    const found = messages(
      checkStyles(
        ".a { transition: color 150ms ease-out; }\n.b { animation: pulse 1500ms infinite; }",
      ),
    );
    expect(found).toEqual([
      "transition: 150ms is not a motion token",
      "transition: only transform and opacity move",
      "animation: 1500ms is not a motion token",
    ]);
  });

  test("tokens, transform and opacity pass", () => {
    expect(
      checkScript(
        [
          '<div className="transition-transform duration-base ease-out ease-in-out" />',
          '<div className="transition-[opacity,transform] [transition:opacity_var(--motion-duration-fast)]" />',
          '<div style={{ transition: "opacity var(--motion-duration-fast) var(--ease-out)" }} />',
          "// transition-colors in a comment is fine",
        ].join("\n"),
      ),
    ).toEqual([]);
    expect(
      checkStyles(
        ".a { animation: pulse var(--motion-duration-pulse) var(--motion-ease) infinite; }",
      ),
    ).toEqual([]);
  });
});
