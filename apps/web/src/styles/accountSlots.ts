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
