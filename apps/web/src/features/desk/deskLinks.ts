import type { DeskElsewhere } from "./api";

type ElsewhereCount = Exclude<keyof DeskElsewhere, "screener_account">;

interface LinkBase {
  key: ElsewhereCount;
  label: string;
  count: number;
  suffix?: string;
}

export type ElsewhereLink = LinkBase &
  (
    | { to: "/reading" | "/updates" | "/deliveries" | "/invites" }
    | { to: "/screener"; search: { account?: string } }
  );

export function elsewhereLinks(counts: DeskElsewhere): ElsewhereLink[] {
  const links: ElsewhereLink[] = [
    {
      key: "reading",
      label: "Reading",
      count: counts.reading,
      suffix: "this week",
      to: "/reading",
    },
    {
      key: "paper_trail",
      label: "Updates",
      count: counts.paper_trail,
      suffix: "this week",
      to: "/updates",
    },
    { key: "deliveries", label: "Deliveries", count: counts.deliveries, to: "/deliveries" },
    { key: "invites", label: "Invites", count: counts.invites, to: "/invites" },
    {
      key: "screener",
      label: "Screener",
      count: counts.screener,
      to: "/screener",
      search: { account: counts.screener_account ?? undefined },
    },
  ];
  return links.filter((link) => link.count > 0);
}
