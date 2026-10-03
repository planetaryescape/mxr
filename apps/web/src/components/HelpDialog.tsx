import { useMemo, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ModeHelp, modeOfScopes } from "@/features/modes/ModeHelp";
import { type ShortcutHint, useActionContext, useShortcutSections } from "@/lib/actions";

interface HelpDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Keyboard reference generated from the action registry: the current
 * view's keys first, then mail actions, global keys, and other views.
 * Keys that differ from the TUI say so inline.
 */
export function HelpDialog({ open, onOpenChange }: HelpDialogProps) {
  const [query, setQuery] = useState("");
  const ctx = useActionContext();
  const sections = useShortcutSections(ctx);
  const mode = modeOfScopes(ctx.scopes);
  const normalized = query.trim().toLowerCase();
  const visible = useMemo(() => {
    if (!normalized) return sections;
    return sections
      .map((section) => ({
        ...section,
        hints: section.hints.filter((hint) =>
          `${section.title} ${hint.keys.join(" ")} ${hint.label} ${hint.note ?? ""}`
            .toLowerCase()
            .includes(normalized),
        ),
      }))
      .filter((section) => section.hints.length > 0);
  }, [normalized, sections]);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setQuery("");
        onOpenChange(next);
      }}
    >
      <DialogContent className="flex max-h-[85dvh] max-w-4xl flex-col gap-3">
        <DialogHeader>
          <DialogTitle>Keyboard</DialogTitle>
          <DialogDescription>
            Keys follow the mxr TUI. Type to filter; Esc closes.
          </DialogDescription>
        </DialogHeader>
        {mode ? <ModeHelp mode={mode} /> : null}
        <Input
          autoFocus
          aria-label="Filter shortcuts"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="archive, label, g i…"
          className="h-9"
        />
        <div
          className="min-h-0 flex-1 overflow-auto rounded-sm pr-1 focus-visible:outline-2 focus-visible:outline-ring"
          // Scrollable, so it must be reachable by keyboard (arrow keys scroll it).
          tabIndex={0}
          role="region"
          aria-label="Shortcuts"
        >
          {visible.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">No matching shortcuts.</p>
          ) : (
            <div className="columns-1 gap-8 md:columns-2">
              {visible.map((section) => (
                <section key={section.id} className="mb-5 break-inside-avoid">
                  <h2 className="mb-1.5 border-b border-border pb-1 font-mono text-2xs uppercase tracking-wider text-muted-foreground">
                    {section.title}
                  </h2>
                  <ul>
                    {section.hints.map((hint) => (
                      <HelpRow key={`${section.id}-${hint.id}`} hint={hint} />
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function HelpRow({ hint }: { hint: ShortcutHint }) {
  return (
    <li className="flex items-start gap-3 py-1">
      <span className="flex w-28 shrink-0 flex-wrap gap-1">
        {hint.keys.slice(0, 2).map((key) => (
          <KeyChip key={key}>{key}</KeyChip>
        ))}
      </span>
      <span className="min-w-0 text-[13px] leading-5">
        {/* Keys that don't apply here step back in colour, not opacity: a
            faded muted note would fall below 4.5:1. */}
        <span className={hint.live ? "text-foreground" : "text-muted-foreground"}>
          {hint.label}
        </span>
        {hint.note ? (
          <span className="block text-2xs leading-4 text-muted-foreground">{hint.note}</span>
        ) : null}
      </span>
    </li>
  );
}
