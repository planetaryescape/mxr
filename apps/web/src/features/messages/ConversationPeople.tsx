import { useQuery } from "@tanstack/react-query";
import { ChevronDown, Users } from "lucide-react";
import emailAddresses from "email-addresses";
import { useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { fetchThread } from "@/features/mailbox/api";
import type { AddressView } from "@/features/mailbox/types";
import { pressedOverlayKey } from "@/lib/keys/overlayKeys";

import { CopyableAddress } from "./AddressDisclosure";

export function ConversationPeople({
  threadId,
  trailing,
}: {
  threadId: string;
  trailing?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId),
    enabled: open,
    staleTime: 60_000,
  });

  const people = thread.data ? recordedPeople(thread.data.messages) : [];

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="min-w-0 w-full">
        <div className="flex items-center justify-end gap-2 px-4">
          {trailing}
          <CollapsibleTrigger asChild>
            <Button
              ref={trigger}
              type="button"
              variant="ghost"
              size="xs"
              aria-label="People in this conversation"
            >
              <Users className="size-3.5" aria-hidden />
              People
              <ChevronDown className="size-3" aria-hidden />
            </Button>
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent>
          <section
            aria-label="People in this conversation"
            data-testid="conversation-people"
            className="mb-2 max-h-[min(45dvh,20rem)] w-full overflow-y-auto border-y border-rule-soft px-4 py-2"
            onKeyDown={(event) => {
              if (!pressedOverlayKey(event, "popover.close")) return;
              event.preventDefault();
              setOpen(false);
              requestAnimationFrame(() => trigger.current?.focus());
            }}
          >
            <p className="type-meta max-w-[66ch] text-text-3">
              These addresses appear in recorded messages. Each message can have a different
              audience; Message info shows the recorded roles for that message.
            </p>
            {thread.isPending ? (
              <p role="status" className="type-meta mt-2 text-text-3">
                Loading people…
              </p>
            ) : thread.isError ? (
              <div className="mt-2 flex items-center gap-2" role="alert">
                <p className="type-meta text-danger">Couldn't load people for this conversation.</p>
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  onClick={() => void thread.refetch()}
                >
                  Retry
                </Button>
              </div>
            ) : people.length > 0 ? (
              <ul className="mt-2 grid min-w-0 grid-cols-1 gap-x-4 gap-y-2 sm:grid-cols-2">
                {people.map((person) => (
                  <li key={person.email} className="min-w-0">
                    {person.name ? (
                      <p className="type-meta truncate text-text-2">{person.name}</p>
                    ) : null}
                    <CopyableAddress address={person.email} />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="type-meta mt-2 text-text-3">
                No addresses are available for these messages.
              </p>
            )}
          </section>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

function recordedPeople(
  messages: {
    sender: string;
    sender_detail?: string | null;
    to?: AddressView[];
    cc?: AddressView[];
    bcc?: AddressView[];
  }[],
): AddressView[] {
  const people = new Map<string, AddressView>();
  const add = (address: AddressView | undefined) => {
    const email = address?.email.trim();
    if (!email || emailAddresses.parseOneAddress(email) === null) return;
    const key = email.toLocaleLowerCase();
    if (!people.has(key)) people.set(key, { ...address, email });
  };

  for (const message of messages) {
    const senderEmail =
      message.sender_detail?.trim() || (message.sender.includes("@") ? message.sender : "");
    add({
      email: senderEmail,
      name: senderEmail && senderEmail !== message.sender ? message.sender : undefined,
    });
    message.to?.forEach(add);
    message.cc?.forEach(add);
    message.bcc?.forEach(add);
  }

  return [...people.values()].toSorted((left, right) =>
    (left.name ?? left.email).localeCompare(right.name ?? right.email),
  );
}
