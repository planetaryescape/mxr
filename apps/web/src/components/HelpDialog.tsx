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
import { shortcutSections, type ShortcutHint, type ShortcutSection } from "@/lib/shortcutHints";
import type { MailPane } from "@/state/mailboxPaneStore";

interface HelpDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  path: string;
  activePane: MailPane;
}

interface HelpRow extends ShortcutHint {
  section: string;
}

export function HelpDialog({ open, onOpenChange, path, activePane }: HelpDialogProps) {
  const [query, setQuery] = useState("");
  const sections = useMemo(() => shortcutSections({ path, activePane }), [activePane, path]);
  const rows = useMemo(() => flattenSections(sections), [sections]);
  const filtered = filterRows(rows, query);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>Help</DialogTitle>
          <DialogDescription>
            Contextual keyboard reference. Press ? again or Esc to close.
          </DialogDescription>
        </DialogHeader>
        <Input
          aria-label="Search help"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search shortcuts, actions, or screens"
          className="h-9"
        />
        <div className="max-h-[62vh] overflow-auto rounded-xl border border-border bg-surface p-3">
          {query.trim() ? (
            <HelpRows rows={filtered} />
          ) : (
            <div className="grid gap-4 md:grid-cols-2">
              {sections.map((section) => (
                <HelpSectionView key={section.title} section={section} />
              ))}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function HelpSectionView({ section }: { section: ShortcutSection }) {
  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold text-foreground">{section.title}</h2>
      <HelpRows rows={section.hints.map((hint) => ({ ...hint, section: section.title }))} />
    </section>
  );
}

function HelpRows({ rows }: { rows: HelpRow[] }) {
  if (rows.length === 0) {
    return <div className="text-xs text-muted-foreground">No matching shortcuts.</div>;
  }
  return (
    <div className="divide-y divide-border">
      {rows.map((row) => (
        <div
          key={`${row.section}-${row.key}-${row.label}`}
          className="flex items-center gap-3 py-2"
        >
          <div className="w-24 shrink-0">
            <KeyChip>{row.key}</KeyChip>
          </div>
          <div className="min-w-0">
            <div className="truncate text-xs font-medium text-foreground">{row.label}</div>
            <div className="truncate text-2xs text-muted-foreground">{row.section}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

function flattenSections(sections: ShortcutSection[]): HelpRow[] {
  return sections.flatMap((section) =>
    section.hints.map((hint) => ({ ...hint, section: section.title })),
  );
}

function filterRows(rows: HelpRow[], query: string): HelpRow[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return rows;
  return rows.filter((row) =>
    `${row.section} ${row.key} ${row.label}`.toLowerCase().includes(normalized),
  );
}
