import { formatReceived, formatWhen } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";

/** `formatWhen`, kept current as time passes ("Just now" becomes "5m"). */
export function useWhen(value: string | Date | null | undefined): string {
  return useClockLabel((now) => formatWhen(value, now));
}

/** A date's words, kept current; only this text re-renders on a tick. */
export function When({ value }: { value: string | Date | null | undefined }) {
  return useWhen(value);
}

/** `formatReceived`, kept current: "2h ago" moves on while the email stays open. */
export function useReceived(value: string | Date | null | undefined): string {
  return useClockLabel((now) => formatReceived(value, now));
}

/** When an open email arrived, always shown in full (D148). */
export function Received({ value }: { value: string | Date | null | undefined }) {
  return useReceived(value);
}
