import type { AccountIdentity } from "@/features/accounts/useAccountIdentities";
import { cn } from "@/lib/utils";
import type { AccountSlot } from "@/styles/accountSlots";

/*
 * Spelled out per slot so Tailwind sees each class. The palette is for
 * accounts only (docs/blueprint/33): nothing else in the app uses these.
 */
const SLOT_CLASS: Record<AccountSlot, string> = {
  sky: "bg-account-sky text-account-sky-ink",
  amber: "bg-account-amber text-account-amber-ink",
  green: "bg-account-green text-account-green-ink",
  yellow: "bg-account-yellow text-account-yellow-ink",
  blue: "bg-account-blue text-account-blue-ink",
  vermilion: "bg-account-vermilion text-account-vermilion-ink",
  purple: "bg-account-purple text-account-purple-ink",
  slate: "bg-account-slate text-account-slate-ink",
};

export type AccountMarkSize = "row" | "header";

/**
 * The account's chip: its tag on its colour. The accessible name is the
 * account's name, so a screen reader hears "Work" where a sighted reader sees
 * `W`. The tooltip gives the address too.
 */
export function AccountMark({
  identity,
  size = "row",
  className,
}: {
  identity: Pick<AccountIdentity, "name" | "email" | "tag" | "slot">;
  size?: AccountMarkSize;
  className?: string;
}) {
  return (
    <span
      role="img"
      aria-label={identity.name}
      title={`${identity.name} · ${identity.email}`}
      data-account-slot={identity.slot}
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-md font-semibold leading-none tabular-nums",
        SLOT_CLASS[identity.slot],
        size === "row" ? "h-4 min-w-4 px-1 text-2xs" : "h-5 min-w-5 px-1.5 text-xs",
        className,
      )}
    >
      {identity.tag}
    </span>
  );
}

export const SWATCH_CLASS: Record<AccountSlot, string> = {
  sky: "bg-account-sky",
  amber: "bg-account-amber",
  green: "bg-account-green",
  yellow: "bg-account-yellow",
  blue: "bg-account-blue",
  vermilion: "bg-account-vermilion",
  purple: "bg-account-purple",
  slate: "bg-account-slate",
};

/** The accounts' colours side by side: "All accounts" in the switcher. Decorative; each account's name is on its own row. */
export function AccountStrip({
  items,
}: {
  items: readonly { accountId: string; slot: AccountSlot }[];
}) {
  return (
    <span aria-hidden className="inline-flex shrink-0 items-center gap-0.5">
      {items.map((item) => (
        <span key={item.accountId} className={cn("h-3 w-1 rounded-sm", SWATCH_CLASS[item.slot])} />
      ))}
    </span>
  );
}
