import { LowTide } from "@/features/low-tide/LowTide";

/** A place you swept clear: the small low tide, once per clearing. */
export function SweptClear({ place }: { place: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
      <LowTide size="small" line={`${place} is clear.`} className="w-full">
        Low tide. Whatever arrives next gathers here.
      </LowTide>
    </div>
  );
}
