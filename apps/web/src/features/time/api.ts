import { apiFetch } from "@/api/client";
import type { components, operations } from "@/api/generated";

type ResolveOperation = operations["mail_time_resolve"];
/** The route's query, from the generated OpenAPI contract. */
type ResolveQuery = ResolveOperation["parameters"]["query"];

export type TimeResolution = components["schemas"]["TimeResolution"];
export type TimeChoice = components["schemas"]["TimeChoice"];
export type TimeResolveError = components["schemas"]["TimeResolveError"];
export type TimeSpan = components["schemas"]["TimeSpan"];

/**
 * The route's `ResolvedTime` answer, taken from the generated OpenAPI
 * contract so it can't drift from the Rust type. Exactly one of
 * `resolution` and `error` is set.
 */
export type ResolvedTime = Extract<
  ResolveOperation["responses"][200]["content"]["application/json"],
  { kind: "ResolvedTime" }
>;

/**
 * Resolve a time phrase with the daemon's parser, the one the CLI and TUI
 * use, in the daemon's local zone with the user's snooze hours.
 */
export function resolveTime(input: string, signal?: AbortSignal): Promise<ResolvedTime> {
  const params: ResolveQuery = { input, time_zone: browserTimeZone() };
  const query = new URLSearchParams(params);
  return apiFetch<ResolvedTime>(`/api/v1/mail/time/resolve?${query.toString()}`, { signal });
}

/**
 * The browser's IANA zone. Sent with every resolution because the browser
 * may not be on the daemon's machine (`mxr web --remote-host`), and "9am"
 * means 9am where the user is.
 */
export function browserTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
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
