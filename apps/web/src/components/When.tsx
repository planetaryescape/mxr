import { formatReceived, formatWhen } from "@/lib/format";
import { useClockLabel } from "@/lib/minuteClock";

export function useWhen(value: string | Date | null | undefined): string {
  return useClockLabel((now) => formatWhen(value, now));
}

export function When({ value }: { value: string | Date | null | undefined }) {
  return useWhen(value);
}

export function useReceived(value: string | Date | null | undefined): string {
  return useClockLabel((now) => formatReceived(value, now));
}

export function Received({ value }: { value: string | Date | null | undefined }) {
  return useReceived(value);
}
