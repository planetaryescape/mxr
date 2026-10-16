/* @vitest-environment node */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { blend, differenceEuclidean, parse, wcagContrast } from "culori";
import { describe, expect, test } from "vitest";

import { generateIncreased, generateRoles } from "./themeGenerator";
import { renderThemesCss, THEMES } from "./themes";

/*
 * Spike S3 (docs/blueprint/31, D139): the generator, given Midnight's three
 * inputs, reproduces today's hand-tuned Midnight within ΔE OK 2. Midnight's
 * values as committed before the generator existed are frozen below; they
 * are the reference, so retuning a rule that drifts Midnight fails here.
 */

/** Midnight in `tokens.css` at 1e0bbc3c5, every var() and color-mix() resolved. */
const MIDNIGHT_BEFORE: Record<string, string> = {
  "ink-room": "#080f17",
  "ink-desk": "#0d161f",
  "ink-lamp": "#17202a",
  "ink-overlay": "#1f2a35",
  scrim: "rgb(4 9 14 / 0.55)",
  "text-1": "#e2e9f0",
  "text-2": "#a6b2c0",
  "text-3": "#808fa0",
  signal: "#5cd5fc",
  "signal-soft": "#01394a",
  cue: "#f8ca65",
  danger: "#fd8a8c",
  ok: "#68d7a1",
  "rule-soft": "#202a34",
  rule: "#2b3742",
  background: "#0d161f",
  foreground: "#e2e9f0",
  "fg-2": "#a6b2c0",
  surface: "#17202a",
  "surface-elevated": "#1f2a35",
  card: "#17202a",
  "card-foreground": "#e2e9f0",
  popover: "#1f2a35",
  "popover-foreground": "#e2e9f0",
  muted: "#1c2632",
  "muted-foreground": "#a6b2c0",
  subtle: "#232f3c",
  "subtle-foreground": "#808fa0",
  faint: "#808fa0",
  border: "#2b3742",
  "border-strong": "#424f5c",
  "input-border": "#687887",
  input: "#1c2632",
  ring: "#54ddff",
  primary: "#5cd5fc",
  "primary-foreground": "#031826",
  "primary-muted": "#01394a",
  secondary: "#232f3c",
  "secondary-foreground": "#d3dce5",
  accent: "#1c2632",
  "accent-foreground": "#e2e9f0",
  destructive: "#fd8a8c",
  "destructive-foreground": "#1c0508",
  success: "#68d7a1",
  "success-foreground": "#04170d",
  warning: "#f8ca65",
  "warning-foreground": "#1d1500",
  vip: "#f8ca65",
  "vip-foreground": "#1d1500",
  pending: "#baaaf8",
  "pending-foreground": "#120a2b",
  "unread-marker": "#5cd5fc",
  star: "#f8ca65",
  "ask-mark": "rgb(248 202 101 / 0.18)",
  "ask-mark-rule": "rgb(248 202 101 / 0.85)",
  "thread-line": "#424f5c",
  selection: "#1e2e3a",
  sidebar: "#080f17",
  "sidebar-foreground": "#a6b2c0",
  "sidebar-primary": "#5cd5fc",
  "sidebar-primary-foreground": "#031826",
  "sidebar-accent": "#17202a",
  "sidebar-accent-foreground": "#e2e9f0",
  "sidebar-border": "#202a34",
  "sidebar-ring": "#5cd5fc",
  "chart-1": "#5cd5fc",
  "chart-2": "#baaaf8",
  "chart-3": "#68d7a1",
  "chart-4": "#f8ca65",
  "chart-5": "#ff9e7a",
  "chart-6": "#6fe3d6",
};

/** The contrast-increased override Midnight shipped with. */
const MIDNIGHT_INCREASED_BEFORE: Record<string, string> = {
  "muted-foreground": "#c4ced8",
  "subtle-foreground": "#b2bfcc",
  border: "#52616f",
  "border-strong": "#728190",
  "input-border": "#728190",
  input: "#202d39",
  ring: "#8ae9ff",
};

const MAX_DELTA = 2;
const deltaOk = differenceEuclidean("oklab");

/** Resolve `var(--x)` through the same theme, so aliases compare as colours. */
function resolve(roles: Map<string, string>, name: string): string {
  const value = roles.get(name);
  if (value === undefined) throw new Error(`no role --${name}`);
  const alias = /^var\(--([\w-]+)\)$/.exec(value);
  return alias?.[1] ? resolve(roles, alias[1]) : value;
}

function colour(css: string) {
  const parsed = parse(css);
  if (!parsed) throw new Error(`cannot parse ${css}`);
  return parsed;
}

/** A translucent colour is compared as it lands on the desk. */
function onDesk(css: string, desk: string) {
  const c = colour(css);
  return (c.alpha ?? 1) < 1 ? blend([colour(desk), c], "normal") : c;
}

function delta(css: string, other: string, desk: string): number {
  return deltaOk(onDesk(css, desk), onDesk(other, desk)) * 100;
}

const midnight = THEMES.find((theme) => theme.id === "midnight")!;

describe("the generated Midnight (spike S3)", () => {
  const roles = generateRoles(midnight);

  test("every role is within ΔE OK 2 of the committed Midnight", () => {
    const misses: string[] = [];
    for (const [name, before] of Object.entries(MIDNIGHT_BEFORE)) {
      const d = delta(resolve(roles, name), before, MIDNIGHT_BEFORE["ink-desk"]!);
      if (d > MAX_DELTA)
        misses.push(`--${name}: ${before} -> ${resolve(roles, name)} (ΔE ${d.toFixed(2)})`);
    }
    expect(misses).toEqual([]);
  });

  test("the generator defines every role Midnight had", () => {
    expect(Object.keys(MIDNIGHT_BEFORE).filter((name) => !roles.has(name))).toEqual([]);
  });

  test("contrast-increased roles are within ΔE OK 2 as well", () => {
    const increased = generateIncreased(midnight);
    const misses = Object.entries(MIDNIGHT_INCREASED_BEFORE).filter(
      ([name, before]) =>
        delta(resolve(increased, name), before, MIDNIGHT_BEFORE["ink-desk"]!) > MAX_DELTA,
    );
    expect(misses.map(([name]) => name)).toEqual([]);
  });
});

describe.each(THEMES)("$id as generated", (theme) => {
  const roles = generateRoles(theme);
  const desk = resolve(roles, "ink-desk");
  const ratio = (fg: string, bg: string) =>
    wcagContrast(colour(resolve(roles, fg)), colour(resolve(roles, bg)));

  test("the text tiers meet their contrast targets on the desk", () => {
    // The solver stops within sRGB rounding of the target.
    const short = ["text-1", "text-2", "text-3"].filter(
      (tier, i) => ratio(tier, "ink-desk") < theme.contrast[i]! - 0.08,
    );
    expect(short).toEqual([]);
  });

  test("text on a filled control passes 4.5:1", () => {
    const pairs = [
      ["primary-foreground", "primary"],
      ["destructive-foreground", "destructive"],
      ["success-foreground", "success"],
      ["warning-foreground", "warning"],
      ["pending-foreground", "pending"],
      ["unread-marker-foreground", "unread-marker"],
    ] as const;
    expect(pairs.filter(([fg, bg]) => ratio(fg, bg) < 4.5)).toEqual([]);
  });

  test("status colours and the accent read as text on the desk (4.5:1)", () => {
    const low = ["signal", "cue", "danger", "ok", "pending"].filter(
      (name) => ratio(name, "ink-desk") < 4.5,
    );
    expect(low).toEqual([]);
  });

  test("contrast-increased text beats the default tiers", () => {
    const increased = generateIncreased(theme);
    const on = (name: string) => wcagContrast(colour(resolve(increased, name)), colour(desk));
    expect(on("muted-foreground")).toBeGreaterThan(ratio("text-2", "ink-desk"));
    expect(on("subtle-foreground")).toBeGreaterThan(ratio("text-3", "ink-desk"));
    expect(on("subtle-foreground")).toBeGreaterThanOrEqual(7);
    expect(on("input-border")).toBeGreaterThanOrEqual(3);
  });
});

test("Paper's unread marks and count badges use the text tier, not rust (D139)", () => {
  const paper = generateRoles(THEMES.find((theme) => theme.id === "paper")!);
  expect(paper.get("unread-marker")).toBe("var(--text-1)");
  expect(paper.get("unread-marker-foreground")).toBe("var(--ink-desk)");
  const midnightRoles = generateRoles(midnight);
  expect(midnightRoles.get("unread-marker")).toBe("var(--signal)");
});

test("themes.generated.css is current (run `npm run gen:themes`)", () => {
  const committed = readFileSync(
    fileURLToPath(new URL("./themes.generated.css", import.meta.url)),
    "utf8",
  );
  expect(committed).toBe(renderThemesCss());
});
