/* @vitest-environment node */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, test } from "vitest";

/*
 * docs/blueprint/27: "Faint is the floor for any text: every tier passes
 * 4.5:1 on room, desk and lamp." This reads the role values out of
 * tokens.css, so editing a theme without checking contrast fails here.
 */

const css = readFileSync(fileURLToPath(new URL("./tokens.css", import.meta.url)), "utf8");

const THEMES = {
  midnight: ':root,\n[data-theme="dark"],\n[data-theme="midnight"] {',
  light: '[data-theme="light"] {',
  eclipse: '[data-theme="eclipse"] {',
  paper: '[data-theme="paper"] {',
} as const;

type Theme = keyof typeof THEMES;

function block(theme: Theme): string {
  const start = css.indexOf(THEMES[theme]);
  if (start < 0) throw new Error(`no ${theme} block in tokens.css`);
  return css.slice(start, css.indexOf("\n}\n", start));
}

function role(theme: Theme, name: string): string {
  const match = new RegExp(`^ {2}--${name}: ([^;]+);`, "m").exec(block(theme));
  if (!match?.[1]) throw new Error(`${theme} has no --${name}`);
  return match[1];
}

const linear = (channel: number) =>
  channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
const clamp = (value: number) => Math.max(0, Math.min(1, value));

/** Linear sRGB from a hex, hsl() or oklch() colour as written in tokens.css. */
function toLinearRgb(value: string): [number, number, number] {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex?.[1]) {
    const n = parseInt(hex[1], 16);
    return [linear((n >> 16) / 255), linear(((n >> 8) & 255) / 255), linear((n & 255) / 255)];
  }
  const hsl = /^hsl\((\d+(?:\.\d+)?) (\d+(?:\.\d+)?)% (\d+(?:\.\d+)?)%\)$/.exec(value);
  if (hsl) {
    const [h, s, l] = [Number(hsl[1]), Number(hsl[2]) / 100, Number(hsl[3]) / 100];
    const a = s * Math.min(l, 1 - l);
    const f = (n: number) => {
      const k = (n + h / 30) % 12;
      return l - a * Math.max(-1, Math.min(k - 3, Math.min(9 - k, 1)));
    };
    return [linear(f(0)), linear(f(8)), linear(f(4))];
  }
  const oklch = /^oklch\((\d+(?:\.\d+)?)% (\d+(?:\.\d+)?) (\d+(?:\.\d+)?)\)$/.exec(value);
  if (oklch) {
    const L = Number(oklch[1]) / 100;
    const a = Number(oklch[2]) * Math.cos((Number(oklch[3]) * Math.PI) / 180);
    const b = Number(oklch[2]) * Math.sin((Number(oklch[3]) * Math.PI) / 180);
    const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
    const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
    const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
    return [
      clamp(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
      clamp(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
      clamp(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
    ];
  }
  throw new Error(`unsupported colour ${value}`);
}

function luminance(value: string): number {
  const [r, g, b] = toLinearRgb(value);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(foreground: string, background: string): number {
  const [a, b] = [luminance(foreground), luminance(background)];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

describe("the contrast helper", () => {
  test("black on white is 21:1 and a colour on itself is 1:1", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrast("#5a6e80", "#5a6e80")).toBeCloseTo(1, 5);
  });

  test("matches the figures in docs/blueprint/27 for Midnight's faint text", () => {
    expect(contrast("#808fa0", "#0d161f")).toBeCloseTo(5.52, 1);
    expect(contrast("#808fa0", "#17202a")).toBeCloseTo(4.98, 1);
  });

  test("reads hsl() and oklch() values", () => {
    expect(contrast("hsl(0 0% 0%)", "hsl(0 0% 100%)")).toBeCloseTo(21, 3);
    expect(contrast("oklch(0% 0 0)", "oklch(100% 0 0)")).toBeCloseTo(21, 1);
  });
});

describe.each(Object.keys(THEMES) as Theme[])("%s text tiers", (theme) => {
  test.each(["ink-room", "ink-desk", "ink-lamp"])(
    "text-1, text-2 and text-3 pass 4.5:1 on %s",
    (ink) => {
      for (const tier of ["text-1", "text-2", "text-3"]) {
        const ratio = contrast(role(theme, tier), role(theme, ink));
        if (ratio < 4.5)
          throw new Error(`${theme} ${tier} on ${ink} is ${ratio.toFixed(2)}:1, under 4.5:1`);
        expect(ratio).toBeGreaterThanOrEqual(4.5);
      }
    },
  );

  test("the tiers step down: text-1 > text-2 > text-3 on the desk", () => {
    const on = (tier: string) => contrast(role(theme, tier), role(theme, "ink-desk"));
    expect(on("text-1")).toBeGreaterThan(on("text-2"));
    expect(on("text-2")).toBeGreaterThan(on("text-3"));
  });

  test("text-1 and text-2 pass on the overlay (text-3 is for 14px and up there)", () => {
    for (const tier of ["text-1", "text-2"]) {
      expect(contrast(role(theme, tier), role(theme, "ink-overlay"))).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrast(role(theme, "text-3"), role(theme, "ink-overlay"))).toBeGreaterThanOrEqual(4.4);
  });

  test("the old names alias the roles", () => {
    const aliases: Record<string, string> = {
      background: "var(--ink-desk)",
      foreground: "var(--text-1)",
      "muted-foreground": "var(--text-2)",
      faint: "var(--text-3)",
      surface: "var(--ink-lamp)",
      sidebar: "var(--ink-room)",
      primary: "var(--signal)",
    };
    for (const [name, expected] of Object.entries(aliases)) {
      expect(role(theme, name)).toBe(expected);
    }
  });
});
