import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Mail, Send } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  fetchContactAsymmetry,
  fetchContactDecay,
  type ContactRow,
} from "@/features/analytics/api";
import { cn } from "@/lib/utils";
import { useModals } from "@/state/modalStore";

type ComposeStep = "to" | "subject";
type RecipientContact = Required<Pick<ContactRow, "email">> &
  Pick<ContactRow, "display_name" | "inbound" | "outbound">;

export function ComposeLauncher() {
  const navigate = useNavigate();
  const open = useModals((state) => state.composeLauncherOpen);
  const setOpen = useModals((state) => state.setComposeLauncherOpen);
  const [step, setStep] = useState<ComposeStep>("to");
  const [to, setTo] = useState("");
  const [subject, setSubject] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const contacts = useQuery({
    queryKey: ["compose", "recipient-contacts"],
    queryFn: fetchRecipientContacts,
    enabled: open && step === "to",
    staleTime: 5 * 60 * 1000,
  });

  useEffect(() => {
    if (!open) return;
    setStep("to");
    setTo("");
    setSubject("");
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    window.setTimeout(() => inputRef.current?.focus(), 0);
  }, [open, step]);

  function close() {
    setOpen(false);
  }

  function continueFromTo() {
    setStep("subject");
  }

  function openCompose() {
    close();
    void navigate({
      to: "/compose/new",
      search: {
        ...(to.trim() ? { to: to.trim() } : {}),
        ...(subject.trim() ? { subject: subject.trim() } : {}),
      },
    });
  }

  const isToStep = step === "to";
  const value = isToStep ? to : subject;
  const setValue = isToStep ? setTo : setSubject;
  const recipientFragment = isToStep ? lastRecipientFragment(to) : "";
  const recipientSuggestions = useMemo(
    () => matchRecipientSuggestions(contacts.data ?? [], recipientFragment),
    [contacts.data, recipientFragment],
  );
  const ghostCompletion =
    isToStep && recipientSuggestions[0]?.email
      ? recipientGhostCompletion(recipientFragment, recipientSuggestions[0].email)
      : "";

  function acceptRecipientSuggestion(email: string) {
    setTo((current) => replaceLastRecipientFragment(current, email));
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="top-[22vh] translate-y-0 gap-0 overflow-hidden rounded-xl border-border/80 bg-popover/95 p-0 shadow-2xl backdrop-blur sm:max-w-[640px]">
        <DialogTitle className="sr-only">Compose message</DialogTitle>
        <div className="border-b border-border px-4 py-3">
          <div className="flex items-center gap-2 text-sm font-medium">
            {isToStep ? <Mail className="size-4" /> : <Send className="size-4" />}
            {isToStep ? "Who is this for?" : "Subject"}
          </div>
          <div className="mt-1 text-2xs text-muted-foreground">
            {isToStep
              ? "Recipients are optional. Use commas for multiple people."
              : "Optional. Leave blank to write first."}
          </div>
        </div>
        <div className="p-4">
          <div className="relative">
            {isToStep && ghostCompletion ? (
              <div
                className="pointer-events-none absolute inset-x-0 top-0 flex h-11 items-center overflow-hidden rounded-md border border-transparent px-2 py-1 text-sm"
                aria-hidden="true"
              >
                <span className="whitespace-pre text-transparent">{value}</span>
                <span className="truncate text-muted-foreground/70">{ghostCompletion}</span>
              </div>
            ) : null}
            <Input
              ref={inputRef}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                const firstSuggestion = recipientSuggestions[0]?.email;
                const input = event.currentTarget;
                const caretAtEnd =
                  input.selectionStart === input.value.length &&
                  input.selectionEnd === input.value.length;
                if (
                  isToStep &&
                  firstSuggestion &&
                  ghostCompletion &&
                  (event.key === "Tab" || (event.key === "ArrowRight" && caretAtEnd))
                ) {
                  event.preventDefault();
                  acceptRecipientSuggestion(firstSuggestion);
                  return;
                }
                if (event.key === "Enter") {
                  event.preventDefault();
                  if (isToStep) continueFromTo();
                  else openCompose();
                }
              }}
              placeholder={isToStep ? "name@example.com, teammate@example.com" : "Subject"}
              aria-label={isToStep ? "Recipients" : "Subject"}
              aria-autocomplete={isToStep ? "list" : undefined}
              aria-controls={isToStep ? "compose-recipient-suggestions" : undefined}
              className="relative z-10 h-11 bg-transparent text-sm"
            />
          </div>
          {isToStep && recipientSuggestions.length > 0 ? (
            <div
              id="compose-recipient-suggestions"
              className="mt-2 overflow-hidden rounded-md border border-border bg-popover shadow-lg"
              role="listbox"
            >
              {recipientSuggestions.map((contact, index) => (
                <button
                  key={contact.email}
                  type="button"
                  role="option"
                  aria-selected={index === 0}
                  className={cn(
                    "flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-xs hover:bg-muted",
                    index === 0 && "bg-primary/10",
                  )}
                  onClick={() => acceptRecipientSuggestion(contact.email)}
                >
                  <span className="min-w-0">
                    <span className="block truncate font-medium">
                      {contact.display_name || contact.email}
                    </span>
                    <span className="block truncate font-mono text-2xs text-muted-foreground">
                      {contact.email}
                    </span>
                  </span>
                  <span className="shrink-0 font-mono text-2xs text-muted-foreground">Tab</span>
                </button>
              ))}
            </div>
          ) : null}
        </div>
        <div className="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
          <div className="font-mono text-2xs text-muted-foreground">
            Enter {isToStep ? "next" : "compose"} · Esc cancel
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" size="sm" onClick={close}>
              Cancel
            </Button>
            {isToStep ? (
              <Button type="button" size="sm" onClick={continueFromTo}>
                Next
              </Button>
            ) : (
              <Button type="button" size="sm" onClick={openCompose}>
                Compose
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

async function fetchRecipientContacts(): Promise<RecipientContact[]> {
  const [asymmetry, decay] = await Promise.allSettled([
    fetchContactAsymmetry(80),
    fetchContactDecay(80),
  ]);
  const rows = [
    ...(asymmetry.status === "fulfilled" ? asymmetry.value.rows : []),
    ...(decay.status === "fulfilled" ? decay.value.rows : []),
  ];
  const contacts = new Map<string, RecipientContact>();
  for (const row of rows) {
    const email = row.email?.trim();
    if (!email) continue;
    const key = email.toLowerCase();
    if (!contacts.has(key)) contacts.set(key, { ...row, email });
  }
  return [...contacts.values()];
}

function lastRecipientFragment(value: string): string {
  const commaIndex = value.lastIndexOf(",");
  return value.slice(commaIndex + 1).trim();
}

function matchRecipientSuggestions(contacts: RecipientContact[], fragment: string) {
  const needle = fragment.toLowerCase();
  if (needle.length < 2) return [];
  return contacts
    .filter((contact) => {
      const email = contact.email.toLowerCase();
      const name = contact.display_name?.toLowerCase() ?? "";
      return email.includes(needle) || name.includes(needle);
    })
    .toSorted((a, b) => contactScore(b) - contactScore(a))
    .slice(0, 5);
}

function recipientGhostCompletion(fragment: string, email: string): string {
  if (!fragment) return "";
  return email.toLowerCase().startsWith(fragment.toLowerCase()) ? email.slice(fragment.length) : "";
}

function replaceLastRecipientFragment(value: string, email: string): string {
  const commaIndex = value.lastIndexOf(",");
  if (commaIndex === -1) return email;
  const prefix = value.slice(0, commaIndex + 1);
  return `${prefix}${prefix.endsWith(" ") ? "" : " "}${email}`;
}

function contactScore(contact: Pick<ContactRow, "inbound" | "outbound">): number {
  return (contact.inbound ?? 0) + (contact.outbound ?? 0);
}
