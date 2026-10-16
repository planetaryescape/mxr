import { LowTideScene } from "@/features/low-tide/LowTide";

/**
 * The scene for `PageEmpty`'s `moment` slot. Reduced motion shows its still
 * frame (docs/blueprint/26 "Motion", D134): a picture is not motion, and the
 * finish is earned, so the CSS drops the animation and keeps the drawing.
 */
export function FinishingMoment() {
  return <LowTideScene size="small" sound />;
}
