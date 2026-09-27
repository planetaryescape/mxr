import { useQuery } from "@tanstack/react-query";
import { Newspaper, Receipt, ShieldOff } from "lucide-react";

import { messageKindQuery, type SenderKind } from "./api";
import { KIND_LABELS, whyHere } from "./placeCopy";
import { openMoveSenderForKind } from "./placeVerbs";

const ICONS: Partial<Record<SenderKind, typeof Newspaper>> = {
  reading: Newspaper,
  paper_trail: Receipt,
  screened_out: ShieldOff,
};

/**
 * In the reader, for mail that isn't from a person: which place it belongs
 * to and why, with the way to move the sender. Sits where reading ends,
 * so a late answer never shifts the conversation above it.
 */
export function WhyHereLine({
  messageId,
  senderLabel,
}: {
  messageId: string;
  senderLabel: string;
}) {
  const kind = useQuery(messageKindQuery(messageId));
  const data = kind.data;
  if (!data || data.mail_kind.kind === "people") return null;
  const Icon = ICONS[data.mail_kind.kind] ?? Receipt;
  return (
    <p
      data-testid="reader-why-here"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 px-5 pt-4 text-[12px] text-muted-foreground"
    >
      <span className="inline-flex items-center gap-1.5">
        <Icon aria-hidden className="size-3.5 shrink-0" />
        {KIND_LABELS[data.mail_kind.kind]}. {whyHere(data.mail_kind)}
      </span>
      <button
        type="button"
        onClick={() => openMoveSenderForKind(data, senderLabel)}
        className="text-muted-foreground underline decoration-border underline-offset-2 hover:text-foreground"
      >
        Move sender…
      </button>
    </p>
  );
}
