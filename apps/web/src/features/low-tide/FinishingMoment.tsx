import { LowTideScene } from "@/features/low-tide/LowTide";
import { resolveMotion, useUiPrefs } from "@/state/uiPrefsStore";

/**
 * The scene for `PageEmpty`'s `moment` slot. Reduced motion skips it
 * (docs/blueprint/26 "Motion"): the words alone are the finish.
 */
export function FinishingMoment() {
  const motion = useUiPrefs((s) => s.motion);
  if (resolveMotion(motion) === "reduced") return null;
  return <LowTideScene size="small" sound />;
}
