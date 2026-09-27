/*
 * Low tide: the moment a cleared desk, reply queue or place earns. A small
 * drawn shore in the theme's own colours: the water's edge draws back, the
 * wet sand dries, a few tide pools catch the light, and then it is still.
 * Under two seconds, once per clearing, and a still frame (the same final
 * picture) with reduced motion. No points, no streaks, nothing to keep up.
 */

import { useEffect, useRef, type ReactNode } from "react";

import { playSound } from "@/features/sound/player";
import { cn } from "@/lib/utils";

export function LowTide({
  size = "full",
  line,
  children,
  sound = false,
  className,
}: {
  /** "full" for the desk; "small" for a swept place. */
  size?: "full" | "small";
  line: string;
  /** Quiet lines under the main one (what's next). */
  children?: ReactNode;
  /** Play the cleared-desk sound once, if sound is on. */
  sound?: boolean;
  className?: string;
}) {
  return (
    <div
      data-testid="low-tide"
      role="status"
      className={cn(
        "flex flex-col items-center text-center",
        size === "full" ? "gap-4" : "gap-2.5",
        className,
      )}
    >
      <LowTideScene size={size} sound={sound} />
      <div>
        <p
          className={cn(
            "text-balance text-foreground",
            size === "full" ? "text-[17px] font-semibold tracking-tight" : "text-[15px]",
          )}
        >
          {line}
        </p>
        {children ? (
          <div className="mt-1 text-pretty text-[13px] leading-5 text-muted-foreground tabular-nums">
            {children}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** Just the drawing, for a view that writes its own words (focus mode). */
export function LowTideScene({
  size = "full",
  sound = false,
}: {
  size?: "full" | "small";
  sound?: boolean;
}) {
  const played = useRef(false);
  useEffect(() => {
    if (!sound || played.current) return;
    played.current = true;
    playSound("low_tide");
  }, [sound]);
  return <Shore size={size} />;
}

/** The drawing. Every colour is a theme token, so each theme gets its own shore. */
function Shore({ size }: { size: "full" | "small" }) {
  return (
    <svg
      aria-hidden
      viewBox="0 0 480 120"
      preserveAspectRatio="xMidYMid slice"
      data-testid="low-tide-scene"
      className={cn(
        "low-tide block overflow-hidden",
        size === "full" ? "h-28 w-full max-w-[30rem]" : "h-14 w-full max-w-[18rem]",
      )}
    >
      {/* Sand, then the pools the water leaves behind. */}
      <rect width="480" height="120" fill="var(--tide-sand)" />
      <g fill="var(--tide-pool)">
        <ellipse cx="108" cy="92" rx="34" ry="5" />
        <ellipse cx="296" cy="102" rx="46" ry="6" />
        <ellipse cx="408" cy="86" rx="22" ry="3.5" />
      </g>
      {/* Wet sand just below the waterline; it dries as the tide goes. */}
      <path
        className="tide-wet"
        d="M0 58 Q60 52 120 57 T240 55 T360 58 T480 54 V76 Q400 72 320 75 T160 74 T0 77Z"
        fill="var(--tide-wet)"
      />
      {/* The sea, and its edge as a single ruled line. */}
      <g className="tide-water">
        <path d="M0 0 H480 V54 Q420 58 360 55 T240 52 T120 56 T0 52Z" fill="var(--tide-water)" />
        <path d="M0 0 H480 V34 Q400 38 320 35 T160 33 T0 36Z" fill="var(--tide-deep)" />
        <path
          d="M0 52 Q60 56 120 56 T240 52 T360 55 T480 54"
          fill="none"
          stroke="var(--tide-edge)"
          strokeWidth="1"
          vectorEffect="non-scaling-stroke"
        />
      </g>
      {/* Light caught in the pools. */}
      <g className="tide-glints">
        <circle cx="96" cy="91" r="1.6" fill="var(--primary)" />
        <circle cx="310" cy="101" r="2" fill="var(--warning)" />
        <circle cx="286" cy="103" r="1.2" fill="var(--primary)" />
        <circle cx="414" cy="86" r="1.3" fill="var(--primary)" />
      </g>
    </svg>
  );
}
