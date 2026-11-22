import { Link } from "@tanstack/react-router";

import { RAIL_PATHS, railCount, useRailQuery } from "@/features/modes/rail";

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
