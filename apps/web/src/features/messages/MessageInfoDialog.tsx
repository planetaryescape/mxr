import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { fetchThread } from "@/features/mailbox/api";
import type { AddressView } from "@/features/mailbox/types";

import { CopyableAddress } from "./AddressDisclosure";
import type { ConversationMessage } from "./api";

export function MessageInfoDialog({
  threadId,
  message,
}: {
  threadId: string;
  message: ConversationMessage;
}) {
  const [open, setOpen] = useState(false);
  const speaker = message.from_me ? "You" : (message.from.name ?? message.from.email);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          type="button"
          aria-label={`Message info for ${speaker} at ${message.date}`}
          className="type-meta text-text-3 underline decoration-transparent underline-offset-2 hover:text-text-1 hover:decoration-current focus-visible:decoration-current"
        >
          Message info
        </button>
      </DialogTrigger>
      {open ? <MessageInfoContent threadId={threadId} message={message} /> : null}
    </Dialog>
  );
}

function MessageInfoContent({
  threadId,
  message,
}: {
  threadId: string;
  message: ConversationMessage;
}) {
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    staleTime: 60_000,
  });
  const details = thread.data?.messages.find((candidate) => candidate.id === message.message_id);

  return (
    <DialogContent
      className="flex max-h-[85dvh] max-w-md flex-col"
      data-testid="message-info-dialog"
    >
      <DialogHeader>
        <DialogTitle>Message info</DialogTitle>
        <DialogDescription>
          Addresses recorded for this message. An empty or unavailable Bcc list does not confirm
          that nobody was hidden.
        </DialogDescription>
      </DialogHeader>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {thread.isPending ? (
          <p role="status" className="type-meta text-text-3">
            Loading message info…
          </p>
        ) : thread.isError ? (
          <div className="space-y-2" role="alert">
            <p className="type-meta text-danger">Couldn't load message info.</p>
            <Button type="button" variant="outline" size="sm" onClick={() => void thread.refetch()}>
              Retry
            </Button>
          </div>
        ) : details ? (
          <div className="space-y-4">
            <RecipientGroup
              label="From"
              addresses={[{ name: message.from.name, email: message.from.email }]}
            />
            <RecipientGroup label="To" addresses={details.to} />
            <RecipientGroup label="Cc" addresses={details.cc} />
            <RecipientGroup
              label="Bcc"
              addresses={details.bcc}
              emptyText="Not recorded in this copy"
            />
          </div>
        ) : (
          <p role="status" className="type-meta text-text-3">
            Message addresses are unavailable.
          </p>
        )}
      </div>
    </DialogContent>
  );
}

function RecipientGroup({
  label,
  addresses,
  emptyText = "None recorded",
}: {
  label: string;
  addresses?: AddressView[];
  emptyText?: string;
}) {
  const available = addresses?.filter((address) => address.email.trim().length > 0) ?? [];
  const emptyMessage = addresses === undefined || addresses.length > 0 ? "Unavailable" : emptyText;
  const occurrences = new Map<string, number>();
  return (
    <section aria-label={label}>
      <h3 className="type-eyebrow text-text-3">{label}</h3>
      {available.length > 0 ? (
        <ul className="mt-1 divide-y divide-rule-soft">
          {available.map((address) => {
            const identity = `${address.name ?? ""}:${address.email}`;
            const occurrence = occurrences.get(identity) ?? 0;
            occurrences.set(identity, occurrence + 1);
            return (
              <li key={`${identity}:${occurrence}`} className="py-1 first:pt-0 last:pb-0">
                {address.name ? <p className="type-meta text-text-2">{address.name}</p> : null}
                <CopyableAddress address={address.email} />
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="type-meta mt-1 text-text-3">{emptyMessage}</p>
      )}
    </section>
  );
}
