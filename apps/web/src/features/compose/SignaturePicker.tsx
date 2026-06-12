/*
 * Signature picker (cmd+shift+G): Command list over saved signatures
 * (/api/v1/mail/signatures). Selecting one appends a `--` signature block
 * to the message body.
 */

import { PenLine } from "lucide-react";

import {
  CommandDialog,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import type { Signature } from "./useComposeSession";

interface SignaturePickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  signatures: Signature[];
  onInsert: (body: string) => void;
}

export function SignaturePicker({
  open,
  onOpenChange,
  signatures,
  onInsert,
}: SignaturePickerProps) {
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <CommandInput placeholder="Insert signature…" />
      <CommandList>
        <CommandEmpty>
          {signatures.length === 0 ? "No signatures saved yet." : "No matching signatures."}
        </CommandEmpty>
        {signatures.map((signature) => (
          <CommandItem
            key={signature.id}
            value={`${signature.name} ${signature.body}`}
            onSelect={() => onInsert(signature.body)}
          >
            <PenLine className="size-3.5" />
            <span className="min-w-0">
              <span className="block truncate text-xs font-medium">{signature.name}</span>
              <span className="block truncate text-2xs text-muted-foreground">
                {previewLine(signature.body)}
              </span>
            </span>
          </CommandItem>
        ))}
      </CommandList>
    </CommandDialog>
  );
}

function previewLine(body: string): string {
  const line = body.split("\n").find((item) => item.trim()) ?? "";
  return line.length > 96 ? `${line.slice(0, 96)}…` : line;
}
