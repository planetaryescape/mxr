import { formatWhen } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";

/** `formatWhen`, kept current as time passes ("Just now" becomes "5m"). */
export function useWhen(value: string | Date | null | undefined): string {
  return useClockLabel((now) => formatWhen(value, now));
}

/** A date's words, kept current; only this text re-renders on a tick. */
export function When({ value }: { value: string | Date | null | undefined }) {
  return useWhen(value);
}
