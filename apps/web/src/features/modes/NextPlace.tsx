import { Link } from "@tanstack/react-router";

import { RAIL_PATHS, railCount, useRailQuery } from "@/features/modes/rail";

/**
 * "Next: 3 in To do": where the work is once this page has none, from the
 * rail's own counts. Renders nothing when no other place has any, so an empty
 * state never points at another empty one.
 */
export function NextPlace({ current }: { current: string }) {
  const rail = useRailQuery();
  const next = rail.data?.entries.find((entry) => {
    if (entry.id === current || entry.id === "now" || entry.id === "inbox") return false;
    return (railCount(entry)?.value ?? 0) > 0;
  });
  const count = next ? railCount(next) : null;
  if (!next || !count) return null;
  return (
    <>
      {" "}
      Next:{" "}
      <Link to={RAIL_PATHS[next.id] ?? "/now"} className="underline underline-offset-4">
        {count.value.toLocaleString()} in {next.name}
      </Link>
      .
    </>
  );
}
