import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

export type TimeResolution = components["schemas"]["TimeResolution"];
export type TimeChoice = components["schemas"]["TimeChoice"];
export type TimeResolveError = components["schemas"]["TimeResolveError"];
export type TimeSpan = components["schemas"]["TimeSpan"];

/**
 * The daemon's `ResolvedTime` answer, taken from the generated protocol
 * schema so it can't drift from the Rust type. Exactly one of `resolution`
 * and `error` is set.
 */
export type ResolvedTime = Extract<components["schemas"]["ResponseData"], { kind: "ResolvedTime" }>;

/**
 * Resolve a time phrase with the daemon's parser, the one the CLI and TUI
 * use, in the daemon's local zone with the user's snooze hours.
 */
export function resolveTime(input: string, signal?: AbortSignal): Promise<ResolvedTime> {
  const query = new URLSearchParams({ input });
  return apiFetch<ResolvedTime>(`/api/v1/mail/time/resolve?${query.toString()}`, { signal });
}

export function timeResolveQueryKey(input: string) {
  return ["time-resolve", input] as const;
}

/** The reading a bare phrase means: the daemon lists it first. */
export function defaultChoice(answer: ResolvedTime | undefined): TimeChoice | null {
  return answer?.resolution?.choices[0] ?? null;
}

/** "Friday 3 October, 15:00" for toasts and confirmations. */
export function describeChoice(choice: TimeChoice): string {
  return `${choice.date_label}, ${choice.time_label}`;
}
