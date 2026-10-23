import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

import { CopyableAddress } from "./AddressDisclosure";
import type { MessagesRow } from "./api";

export function PersonIdentity({
  row,
  interactive = true,
}: {
  row: MessagesRow;
  interactive?: boolean;
}) {
  const addresses = row.person?.addresses ?? [];
  if (row.kind !== "person") return <span className="min-w-0 truncate">{row.title}</span>;
  if (!interactive) return <span className="min-w-0 truncate">{row.title}</span>;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Show email addresses for ${row.title}`}
          className="min-w-0 truncate text-left text-inherit underline decoration-transparent underline-offset-4 hover:decoration-current focus-visible:decoration-current"
        >
          {row.title}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-[min(22rem,calc(100vw-2rem))]"
        aria-label={`${row.title} email addresses`}
      >
        <p className="type-eyebrow text-text-3">Addresses for {row.title}</p>
        {addresses.length > 0 ? (
          <ul className="mt-2 divide-y divide-rule-soft">
            {addresses.map((address) => (
              <li key={address} className="py-1 first:pt-0 last:pb-0">
                <CopyableAddress address={address} />
              </li>
            ))}
          </ul>
        ) : (
          <p className="type-meta mt-2 text-text-3">Email addresses are unavailable.</p>
        )}
      </PopoverContent>
    </Popover>
  );
}
