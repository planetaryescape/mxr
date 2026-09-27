import type { DeskElsewhere } from "./api";

type ElsewhereCount = Exclude<keyof DeskElsewhere, "screener_account">;

interface LinkBase {
  key: ElsewhereCount;
  label: string;
  count: number;
  suffix?: string;
}

/** Each destination with exactly the params or search it takes. */
export type ElsewhereLink = LinkBase &
  (
    | { to: "/subscriptions" | "/deliveries" | "/invites" }
    | { to: "/m/$mailbox"; params: { mailbox: "inbox" } }
    | { to: "/screener"; search: { account?: string } }
  );

/**
 * The everything-else links. Each page reads the app's account scope, as
 * the desk does, so a count and the page it opens are the same account's.
 * Screener shows one account at a time, so its link names the account.
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
    {
      key: "screener",
      label: "Screener",
      count: counts.screener,
      to: "/screener",
      // An all-accounts desk sums every queue; open one that has senders.
      search: { account: counts.screener_account ?? undefined },
    },
  ];
  return links.filter((link) => link.count > 0);
}
