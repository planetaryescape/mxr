/*
 * The eight account colour slots (docs/blueprint/33). The daemon sends the
 * name, never a hex value, and each client maps it to its own theme. This
 * module has no dependencies so the app can import it without the theme
 * generator, which only the build uses.
 */

export const ACCOUNT_SLOTS = [
  "sky",
  "amber",
  "green",
  "yellow",
  "blue",
  "vermilion",
  "purple",
  "slate",
] as const;
export type AccountSlot = (typeof ACCOUNT_SLOTS)[number];

export function isAccountSlot(value: unknown): value is AccountSlot {
  return typeof value === "string" && (ACCOUNT_SLOTS as readonly string[]).includes(value);
}
