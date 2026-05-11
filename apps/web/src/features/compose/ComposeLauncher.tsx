import { useNavigate } from "@tanstack/react-router";
import { Mail, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useModals } from "@/state/modalStore";

type ComposeStep = "to" | "subject";

export function ComposeLauncher() {
  const navigate = useNavigate();
  const open = useModals((state) => state.composeLauncherOpen);
  const setOpen = useModals((state) => state.setComposeLauncherOpen);
  const [step, setStep] = useState<ComposeStep>("to");
  const [to, setTo] = useState("");
  const [subject, setSubject] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

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
          <Input
            ref={inputRef}
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                if (isToStep) continueFromTo();
                else openCompose();
              }
            }}
            placeholder={isToStep ? "name@example.com, teammate@example.com" : "Subject"}
            aria-label={isToStep ? "Recipients" : "Subject"}
            className="h-11 bg-input text-sm"
          />
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
