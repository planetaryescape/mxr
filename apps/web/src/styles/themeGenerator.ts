/*
 * The theme generator (docs/blueprint/27 "Generating themes", D139). A theme
 * is three inputs, in OKLCH terms: a base (the desk), an accent (the lamp's
 * colour, `--signal`) and a contrast level (the three text tiers' ratios on
 * the desk). Every other role follows from those and from whether the theme
 * is dark or light. What the rules can't say is a short, named override list
 * in themes.ts.
 *
 * The constants below were fitted to Midnight's hand-tuned values (spike S3,
 * docs/blueprint/31): the rules reproduce it within ΔE OK 2, and the other
 * themes inherit the same hierarchy. The output is plain CSS custom
 * properties in `themes.generated.css`, written by `npm run gen:themes`;
 * nothing reads this file in the browser.
 */

import { clampChroma, formatHex, formatRgb, interpolate, oklch, parse, wcagContrast } from "culori";
import type { Oklch } from "culori";

export type ThemeId = "midnight" | "light" | "eclipse" | "paper";

/** Contrast ratios on the desk for text-1, text-2 and text-3 (WCAG 2.1). */
export type Contrast = readonly [number, number, number];

export interface ThemeInput {
  id: ThemeId;
  /** Selector list for the block; Midnight also answers to `:root`, so it is the default. */
  selectors: readonly string[];
  /** Prose for the comment above the block. */
  note: string;
  dark: boolean;
  /** The desk, as any CSS colour. Room, lamp, overlay, rules and surfaces step from it. */
  base: string;
  /** `--signal`: focus, the lit item's hairline, the primary action, links. */
  accent: string;
  contrast: Contrast;
  /**
   * Text hue and per-tier chroma, when the ink should differ from the desk's
   * own tint (Light's navy ink on a pale cyan-grey desk).
   */
  ink?: { hue?: number; chroma?: readonly [number, number, number] };
  /** Unread marks and count badges use the text tier instead of the accent (Paper: rust reads as error red). */
  neutralMarks?: boolean;
  /** Role name to CSS colour. Replaces what the rules would produce; each entry is a documented exception. */
  overrides?: Readonly<Record<string, string>>;
}

/** Shared contrast levels. The numbers are Midnight's and Light's today (spike S3 fit). */
export const CONTRAST = {
  dark: [14.9, 8.5, 5.5],
  light: [14.5, 7.2, 5.0],
  /** Eclipse: lower glare, same hierarchy. */
  soft: [14.2, 8.5, 5.4],
} as const satisfies Record<string, Contrast>;

/** Lightness steps from the desk (additive ΔL in OKLCH), fitted per polarity. */
interface Steps {
  room: number;
  lamp: number;
  overlay: number;
  soft: number;
  rule: number;
  border: number;
  strong: number;
  inputBorder: number;
  muted: number;
  subtle: number;
  accent: number;
  sidebarAccent: number;
  secondary: "subtle" | "muted";
  signalSoft: number;
  ring: number;
  ringHue: number;
  /** Multipliers on the base's chroma. */
  chroma: { room: number; lamp: number; overlay: number; surface: number };
}

const STEPS: Record<"dark" | "light", Steps> = {
  dark: {
    room: -0.03,
    lamp: 0.04,
    overlay: 0.08,
    soft: 0.084,
    rule: 0.135,
    border: 0.135,
    strong: 0.225,
    inputBorder: 0.37,
    muted: 0.069,
    subtle: 0.104,
    accent: 0.069,
    sidebarAccent: 0.04,
    secondary: "subtle",
    signalSoft: 0.13,
    // The focus ring is a touch lighter and cooler than the accent, so it shows on a filled primary control.
    ring: 0.017,
    ringHue: -5,
    chroma: { room: 1, lamp: 1, overlay: 1, surface: 1.15 },
  },
  light: {
    room: -0.027,
    lamp: -0.03,
    // Near-white, never pure white (DESIGN.md).
    overlay: 0.018,
    soft: -0.065,
    rule: -0.139,
    border: -0.065,
    strong: -0.139,
    inputBorder: -0.36,
    muted: -0.031,
    subtle: -0.061,
    accent: -0.021,
    sidebarAccent: -0.048,
    secondary: "muted",
    signalSoft: -0.046,
    ring: 0,
    ringHue: 0,
    chroma: { room: 1.5, lamp: 2, overlay: 0.3, surface: 2.6 },
  },
};

/* Status hues are the same in every theme; lightness and chroma follow the polarity. [L, C, H]. */
type Lch = readonly [number, number, number];
const STATUS = {
  cue: { dark: [0.86, 0.13, 85], light: [0.475, 0.099, 78] },
  danger: { dark: [0.759, 0.14, 20], light: [0.503, 0.179, 23] },
  ok: { dark: [0.799, 0.129, 160], light: [0.515, 0.11, 157] },
  pending: { dark: [0.78, 0.111, 293], light: [0.489, 0.189, 285] },
  orange: { dark: [0.79, 0.126, 41], light: [0.583, 0.146, 35] },
  teal: { dark: [0.845, 0.107, 186], light: [0.551, 0.092, 186] },
  star: { dark: [0.86, 0.13, 85], light: [0.615, 0.129, 76] },
} as const satisfies Record<string, { dark: Lch; light: Lch }>;

/** The ask's quote on a light desk is a highlighter; on a dark desk it is the star colour as a wash. */
const MARK_LIGHT = { fill: [0.953, 0.079, 96], rule: [0.748, 0.153, 85] } as const satisfies Record<
  string,
  Lch
>;

const SCRIM_ALPHA = { dark: 0.55, light: 0.18 } as const;

/** Each text tier's chroma as a multiple of the base's, by polarity. Light ink is a deeper tint of the desk. */
const INK_CHROMA = { dark: [0.5, 1, 1.35], light: [3, 3.4, 3.2] } as const;

/** Contrast-increased mode: targets and steps (explicit preference and `prefers-contrast: more`). */
const INCREASED = {
  dark: {
    muted: 11.4,
    subtle: 9.7,
    border: 0.289,
    strong: 0.4,
    inputBorder: 0.4,
    input: 0.095,
    ring: 0.048,
    // Stronger lines carry more of the desk's tint than the default surfaces do.
    chroma: 1.2,
  },
  light: {
    muted: 8.5,
    subtle: 8.5,
    border: -0.255,
    strong: -0.35,
    inputBorder: -0.4,
    input: -0.031,
    ring: -0.07,
    chroma: 5,
  },
  /** The accent as a fill or text must reach this on the desk. */
  primaryContrast: 6.5,
} as const;

export type Roles = Map<string, string>;

/** An OKLCH colour whose chroma and hue are always present (culori leaves them optional for greys). */
type Color = Oklch & { c: number; h: number };

const withHue = (color: Oklch): Color => ({ ...color, c: color.c ?? 0, h: color.h ?? 0 });

const toOklch = (value: string): Color => {
  const parsed = parse(value);
  if (!parsed) throw new Error(`cannot parse colour ${value}`);
  return withHue(oklch(parsed));
};

const make = (l: number, c: number, h: number, alpha?: number): Color => ({
  mode: "oklch",
  l: Math.min(1, Math.max(0, l)),
  c,
  h,
  ...(alpha === undefined ? {} : { alpha }),
});

const fit = (color: Color): Color => {
  return withHue(clampChroma(color, "oklch") as Oklch);
};

/** A complete value: hex, or `rgb(r g b / a)` when there is alpha. */
function css(color: Color): string {
  const mapped = fit(color);
  if (mapped.alpha === undefined || mapped.alpha >= 1) return formatHex(mapped);
  const [r, g, b] = formatRgb({ ...mapped, alpha: 1 }).match(/\d+/g) ?? [];
  return `rgb(${r} ${g} ${b} / ${mapped.alpha})`;
}

/** The lightness at which `chroma` and `hue` reach `target` contrast on `surface`. */
function solveText(surface: Color, dark: boolean, target: number, c: number, h: number): Color {
  let [lo, hi] = dark ? [surface.l, 1] : [0, surface.l];
  for (let i = 0; i < 60; i++) {
    const mid = (lo + hi) / 2;
    const ratio = wcagContrast(fit(make(mid, c, h)), fit(surface));
    // Dark text sits above the desk, so a low ratio means go lighter; light text, the reverse.
    if (dark ? ratio < target : ratio > target) lo = mid;
    else hi = mid;
  }
  return fit(make((lo + hi) / 2, c, h));
}

/** `color` moved along lightness until it reaches `target` contrast on `surface`, if it is under. */
function reach(color: Color, surface: Color, dark: boolean, target: number): Color {
  if (wcagContrast(fit(color), fit(surface)) >= target) return color;
  return solveText(surface, dark, target, color.c, color.h);
}

/**
 * Text on a status fill: near-black tinted with the fill's hue on dark themes
 * (each fill's [L, C] was fitted to Midnight's), near-white on light ones.
 */
const ON_DARK = {
  accent: [0.2, 0.04],
  cue: [0.2, 0.041],
  danger: [0.16, 0.042],
  ok: [0.184, 0.034],
  pending: [0.18, 0.064],
} as const satisfies Record<string, readonly [number, number]>;

const onColour = (dark: boolean, fill: keyof typeof ON_DARK, hue: number) => {
  if (!dark) return make(0.978, 0.012, hue);
  const [l, c] = ON_DARK[fill];
  return make(l, c, hue);
};

/** Every role for one theme, in output order. */
export function generateRoles(input: ThemeInput): Roles {
  const { dark } = input;
  const s = STEPS[dark ? "dark" : "light"];
  const desk = toOklch(input.base);
  const accent = toOklch(input.accent);
  const step = (dl: number, k = s.chroma.surface) => make(desk.l + dl, desk.c * k, desk.h);

  const room = step(s.room, s.chroma.room);
  const lamp = step(s.lamp, s.chroma.lamp);
  const overlay = step(s.overlay, s.chroma.overlay);

  const inkHue = input.ink?.hue ?? desk.h;
  const inkChroma = input.ink?.chroma ?? INK_CHROMA[dark ? "dark" : "light"].map((k) => k * desk.c);
  const [text1, text2, text3] = input.contrast.map((target, i) =>
    solveText(desk, dark, target, inkChroma[i] ?? 0, inkHue),
  ) as [Color, Color, Color];

  const status = (name: keyof typeof STATUS) => {
    const [l, c, h] = STATUS[name][dark ? "dark" : "light"];
    return make(l, c, h);
  };
  const cue = status("cue");
  const star = status("star");
  const signalSoft = make(
    desk.l + s.signalSoft,
    dark ? accent.c * 0.5 : Math.min(accent.c * 0.25, 0.03),
    accent.h,
  );
  const muted = step(s.muted);
  const subtle = step(s.subtle);
  const rule = step(s.rule);
  const softRule = step(s.soft);
  const border = step(s.border);
  const strong = step(s.strong);

  // Text selection: the accent washed into the lamp on dark themes, the soft accent on light ones.
  const selection = dark
    ? withHue(oklch(interpolate([css(lamp), css(accent)], "oklab")(0.09)))
    : signalSoft;

  const roles: Roles = new Map();
  const set = (name: string, value: string | Color) =>
    roles.set(name, typeof value === "string" ? value : css(value));

  // Roles: the values every other token aliases.
  set("ink-room", room);
  set("ink-desk", desk);
  set("ink-lamp", lamp);
  set("ink-overlay", overlay);
  set(
    "scrim",
    dark
      ? make(room.l - 0.03, desk.c * 0.7, desk.h, SCRIM_ALPHA.dark)
      : { ...text1, alpha: SCRIM_ALPHA.light },
  );
  set("text-1", text1);
  set("text-2", text2);
  set("text-3", text3);
  set("signal", accent);
  set("signal-soft", signalSoft);
  set("cue", cue);
  set("danger", status("danger"));
  set("ok", status("ok"));
  set("rule-soft", softRule);
  set("rule", rule);

  // The names from before the redesign: aliases of the roles, so existing classes keep working.
  set("background", "var(--ink-desk)");
  set("foreground", "var(--text-1)");
  set("fg-2", "var(--text-2)");
  set("surface", "var(--ink-lamp)");
  set("surface-elevated", "var(--ink-overlay)");
  set("card", "var(--ink-lamp)");
  set("card-foreground", "var(--text-1)");
  set("popover", "var(--ink-overlay)");
  set("popover-foreground", "var(--text-1)");

  set("muted", muted);
  set("muted-foreground", "var(--text-2)");
  set("subtle", subtle);
  set("subtle-foreground", "var(--text-3)");
  set("faint", "var(--text-3)");

  set("border", border);
  set("border-strong", strong);
  set("input-border", step(s.inputBorder));
  set("input", muted);
  set("ring", s.ring ? make(accent.l + s.ring, accent.c, accent.h + s.ringHue) : "var(--signal)");

  set("primary", "var(--signal)");
  set("primary-foreground", dark ? onColour(true, "accent", accent.h) : desk);
  set("primary-muted", "var(--signal-soft)");
  set("secondary", s.secondary === "subtle" ? subtle : muted);
  set("secondary-foreground", dark ? make(text1.l - 0.04, text1.c * 1.3, text1.h) : text1);
  set("accent", step(s.accent));
  set("accent-foreground", "var(--text-1)");

  set("destructive", "var(--danger)");
  set("destructive-foreground", onColour(dark, "danger", status("danger").h));
  set("success", "var(--ok)");
  set("success-foreground", onColour(dark, "ok", status("ok").h));
  set("warning", "var(--cue)");
  set("warning-foreground", onColour(dark, "cue", cue.h));
  set("vip", "var(--cue)");
  set("vip-foreground", onColour(dark, "cue", cue.h));
  set("pending", status("pending"));
  set("pending-foreground", onColour(dark, "pending", status("pending").h));

  // Counts and unread marks: the accent, or the primary text tier where the accent reads as an alarm.
  set("unread-marker", input.neutralMarks ? "var(--text-1)" : "var(--signal)");
  set(
    "unread-marker-foreground",
    input.neutralMarks ? "var(--ink-desk)" : "var(--primary-foreground)",
  );
  set("star", star);
  if (dark) {
    // The ask's quote, marked in a message: a soft highlighter, readable text.
    set("ask-mark", { ...star, alpha: 0.18 });
    set("ask-mark-rule", { ...star, alpha: 0.85 });
  } else {
    set("ask-mark", make(...MARK_LIGHT.fill));
    set("ask-mark-rule", make(...MARK_LIGHT.rule));
  }
  set("thread-line", "var(--border-strong)");
  set("selection", selection);

  set("sidebar", "var(--ink-room)");
  set("sidebar-foreground", "var(--text-2)");
  set("sidebar-primary", "var(--signal)");
  set("sidebar-primary-foreground", "var(--primary-foreground)");
  set("sidebar-accent", step(s.sidebarAccent));
  set("sidebar-accent-foreground", "var(--text-1)");
  set("sidebar-border", "var(--rule-soft)");
  set("sidebar-ring", "var(--signal)");

  set("chart-1", "var(--signal)");
  set("chart-2", "var(--pending)");
  set("chart-3", "var(--ok)");
  set("chart-4", "var(--star)");
  set("chart-5", status("orange"));
  set("chart-6", status("teal"));

  for (const [name, value] of Object.entries(input.overrides ?? {})) {
    if (!roles.has(name)) throw new Error(`${input.id}: override of unknown role --${name}`);
    set(name, value.startsWith("var(") ? value : css(toOklch(value)));
  }
  return roles;
}

/** The roles contrast-increased mode changes (explicit preference or `prefers-contrast: more`). */
export function generateIncreased(input: ThemeInput): Roles {
  const { dark } = input;
  const t = INCREASED[dark ? "dark" : "light"];
  const s = STEPS[dark ? "dark" : "light"];
  const desk = toOklch(input.base);
  const accent = toOklch(input.accent);
  const inkHue = input.ink?.hue ?? desk.h;
  const chroma = (INK_CHROMA[dark ? "dark" : "light"][1] ?? 1) * desk.c;
  const step = (dl: number) => make(desk.l + dl, desk.c * t.chroma, desk.h);

  const roles: Roles = new Map();
  const set = (name: string, value: Color) => roles.set(name, css(value));
  set("muted-foreground", solveText(desk, dark, t.muted, input.ink?.chroma?.[1] ?? chroma, inkHue));
  set(
    "subtle-foreground",
    solveText(desk, dark, t.subtle, input.ink?.chroma?.[2] ?? chroma, inkHue),
  );
  set("border", step(t.border));
  set("border-strong", step(t.strong));
  set("input-border", step(t.inputBorder));
  set("input", step(t.input));
  set("ring", make(accent.l + s.ring + t.ring, accent.c, accent.h + s.ringHue));
  const primary = reach(accent, desk, dark, INCREASED.primaryContrast);
  if (css(primary) !== css(accent)) {
    set("primary", primary);
    set("sidebar-primary", primary);
  }
  return roles;
}
