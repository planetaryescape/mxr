/*
 * Snippet picker (cmd+;): fuzzy Command list over the saved snippets the
 * controller already fetches for `;name ` inline expansion. Selecting one
 * appends its body to the end of the message.
 */

import { TextQuote } from "lucide-react";

import {
  CommandDialog,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import type { Snippet } from "./useComposeSession";

interface SnippetPickerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  snippets: Snippet[];
  onInsert: (body: string) => void;
}

export function SnippetPicker({ open, onOpenChange, snippets, onInsert }: SnippetPickerProps) {
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <CommandInput placeholder="Insert snippet…" />
      <CommandList>
        <CommandEmpty>
          {snippets.length === 0 ? "No snippets saved yet." : "No matching snippets."}
        </CommandEmpty>
        {snippets.map((snippet) => (
          <CommandItem
            key={snippet.name}
            value={`${snippet.name} ${snippet.body}`}
            onSelect={() => onInsert(snippet.body)}
          >
            <TextQuote className="size-3.5" />
            <span className="min-w-0">
              <span className="block truncate text-xs font-medium">{snippet.name}</span>
              <span className="block truncate text-2xs text-muted-foreground">
                {previewLine(snippet.body)}
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
