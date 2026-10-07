import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

import { useLedger, type RecordFilter, type RecordKind, type RecordLedger } from "./api";

/** The kinds the bridge accepts; a facet value outside them is skipped. */
const KNOWN_KINDS: readonly RecordKind[] = [
  "receipt",
  "order",
  "booking",
  "invoice",
  "statement",
  "ticket",
  "contract",
  "warranty",
  "account",
];

/** "99.50" to minor units; empty or unreadable is no bound. */
export function toMinor(text: string): number | undefined {
  const cleaned = text.replace(/[^0-9.]/g, "");
  if (!cleaned) return undefined;
  const value = Number(cleaned);
  return Number.isFinite(value) ? Math.round(value * 100) : undefined;
}

const fromMinor = (minor: number | null | undefined) =>
  minor == null ? "" : (minor / 100).toFixed(2);

function Choice({
  pressed,
  onClick,
  children,
}: {
  pressed: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "min-h-8 rounded-full border px-3 text-[12.5px]",
        pressed
          ? "border-primary bg-primary/10 text-foreground"
          : "border-border text-muted-foreground hover:bg-accent hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Filters (`g f`): kind, issuer, year, amount, PDF and checked. Set several,
 * then apply once: the button says how many records the set matches, as
 * NN/g's batch pattern does on a phone, where every reload costs. On a
 * phone it is a tray at the bottom of the screen.
 */
export function FacetsPanel({
  ledger,
  filter,
  tray,
  onApply,
  onClose,
}: {
  ledger: RecordLedger;
  filter: RecordFilter;
  tray: boolean;
  onApply: (filter: RecordFilter) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<RecordFilter>(filter);
  const [min, setMin] = useState(fromMinor(filter.min_amount_minor));
  const [max, setMax] = useState(fromMinor(filter.max_amount_minor));
  const next: RecordFilter = {
    ...draft,
    min_amount_minor: toMinor(min),
    max_amount_minor: toMinor(max),
  };
  const count = useLedger(next, 1);
  const toggleKind = (kind: RecordKind) => {
    const kinds = draft.kinds ?? [];
    setDraft({
      ...draft,
      kinds: kinds.includes(kind) ? kinds.filter((each) => each !== kind) : [...kinds, kind],
    });
  };
  const facets = ledger.facets;
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        data-testid="facets-panel"
        className={cn(
          "max-h-[85dvh] grid-cols-[minmax(0,1fr)] overflow-y-auto",
          tray && "bottom-0 top-auto max-w-none translate-y-0 rounded-t-lg sm:rounded-b-none",
        )}
      >
        <DialogHeader>
          <DialogTitle>Filters</DialogTitle>
          <DialogDescription>Narrow the ledger, then show the records.</DialogDescription>
        </DialogHeader>
        <fieldset className="grid gap-1.5">
          <legend className="mb-1 text-[13px] font-medium">Kind</legend>
          <div className="flex flex-wrap gap-1.5">
            {facets.kinds.flatMap((kind) => {
              const value = KNOWN_KINDS.find((known) => known === kind.value);
              return value
                ? [
                    <Choice
                      key={value}
                      pressed={draft.kinds?.includes(value) ?? false}
                      onClick={() => toggleKind(value)}
                    >
                      {kind.label} {kind.count}
                    </Choice>,
                  ]
                : [];
            })}
          </div>
        </fieldset>
        <fieldset className="grid gap-1.5">
          <legend className="mb-1 text-[13px] font-medium">Year</legend>
          <div className="flex flex-wrap gap-1.5">
            <Choice
              pressed={draft.year == null}
              onClick={() => setDraft({ ...draft, year: undefined })}
            >
              Any
            </Choice>
            {facets.years.map((year) => (
              <Choice
                key={year.value}
                pressed={String(draft.year) === year.value}
                onClick={() => setDraft({ ...draft, year: Number(year.value) })}
              >
                {year.label} {year.count}
              </Choice>
            ))}
          </div>
        </fieldset>
        <label className="grid gap-1 text-[13px]">
          <span className="font-medium">Issuer</span>
          <select
            value={draft.issuer ?? ""}
            onChange={(event) => setDraft({ ...draft, issuer: event.target.value || undefined })}
            className="h-9 rounded-md border border-border bg-background px-2 text-[13px]"
          >
            <option value="">Any</option>
            {facets.issuers.map((issuer) => (
              <option key={issuer.value} value={issuer.label}>
                {issuer.label} ({issuer.count})
              </option>
            ))}
          </select>
        </label>
        <div className="grid grid-cols-2 gap-2 text-[13px]">
          <label className="grid gap-1">
            <span className="font-medium">Amount from</span>
            <Input
              inputMode="decimal"
              value={min}
              placeholder="0"
              onChange={(event) => setMin(event.target.value)}
            />
          </label>
          <label className="grid gap-1">
            <span className="font-medium">to</span>
            <Input
              inputMode="decimal"
              value={max}
              placeholder="any"
              onChange={(event) => setMax(event.target.value)}
            />
          </label>
        </div>
        <div className="flex flex-wrap gap-1.5">
          <Choice
            pressed={draft.has_pdf === true}
            onClick={() =>
              setDraft({ ...draft, has_pdf: draft.has_pdf === true ? undefined : true })
            }
          >
            Has PDF {facets.has_pdf}
          </Choice>
          <Choice
            pressed={draft.checked === true}
            onClick={() =>
              setDraft({ ...draft, checked: draft.checked === true ? undefined : true })
            }
          >
            Checked {facets.checked}
          </Choice>
          <Choice
            pressed={draft.checked === false}
            onClick={() =>
              setDraft({ ...draft, checked: draft.checked === false ? undefined : false })
            }
          >
            Unchecked {facets.unchecked}
          </Choice>
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              setDraft({});
              setMin("");
              setMax("");
            }}
          >
            Clear
          </Button>
          <Button type="button" size="sm" data-testid="facets-apply" onClick={() => onApply(next)}>
            {count.data ? `Show ${plural(count.data.matching, "record")}` : "Show records"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
