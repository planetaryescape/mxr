import { apiFetch } from "@/api/client";
import type { components, operations } from "@/api/generated";

type ResolveOperation = operations["mail_time_resolve"];
type ResolveQuery = ResolveOperation["parameters"]["query"];

export type TimeResolution = components["schemas"]["TimeResolution"];
export type TimeChoice = components["schemas"]["TimeChoice"];
export type TimeResolveError = components["schemas"]["TimeResolveError"];
export type TimeSpan = components["schemas"]["TimeSpan"];

export type ResolvedTime = Extract<
  ResolveOperation["responses"][200]["content"]["application/json"],
  { kind: "ResolvedTime" }
>;

export function resolveTime(input: string, signal?: AbortSignal): Promise<ResolvedTime> {
  const params: ResolveQuery = { input, time_zone: browserTimeZone() };
  const query = new URLSearchParams(params);
  return apiFetch<ResolvedTime>(`/api/v1/mail/time/resolve?${query.toString()}`, { signal });
}

export function browserTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

export function timeResolveQueryKey(input: string) {
  return ["time-resolve", input] as const;
}

export function defaultChoice(answer: ResolvedTime | undefined): TimeChoice | null {
  return answer?.resolution?.choices[0] ?? null;
}

export function describeChoice(choice: TimeChoice): string {
  return `${choice.date_label}, ${choice.time_label}`;
}
