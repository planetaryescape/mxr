import type { DeskElsewhere } from "./api";

export interface ElsewhereLink {
  key: keyof DeskElsewhere;
  label: string;
  count: number;
  suffix?: string;
  to: "/subscriptions" | "/m/$mailbox" | "/deliveries" | "/invites" | "/screener";
  params?: { mailbox: string };
}

/**
 * The everything-else links. Each page reads the app's account scope, as
 * the desk does, so a count and the page it opens are the same account's.
 * Paper trail has no view of its own yet (bundles come later); receipts and
 * notifications sit in the inbox, so it points there.
 */
export function elsewhereLinks(counts: DeskElsewhere): ElsewhereLink[] {
  const links: ElsewhereLink[] = [
    {
      key: "reading",
      label: "Reading",
      count: counts.reading,
      suffix: "new",
      to: "/subscriptions",
    },
    {
      key: "paper_trail",
      label: "Paper trail",
      count: counts.paper_trail,
      to: "/m/$mailbox",
      params: { mailbox: "inbox" },
    },
    { key: "deliveries", label: "Deliveries", count: counts.deliveries, to: "/deliveries" },
    { key: "invites", label: "Invites", count: counts.invites, to: "/invites" },
    { key: "screener", label: "Screener", count: counts.screener, to: "/screener" },
  ];
  return links.filter((link) => link.count > 0);
}
