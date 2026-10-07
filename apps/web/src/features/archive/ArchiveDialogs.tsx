import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";

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

import type { RecordData, RecordFilter } from "./api";
import {
  downloadExport,
  editField,
  fileMessage,
  markChecked,
  previewEdit,
  previewExport,
  previewFile,
} from "./archiveVerbs";
import { Provenance } from "./RecordParts";

function useDebouncedValue(value: string, ms: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), ms);
    return () => window.clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

/** Fields a person can fix, in card order. */
const FIXABLE = [
  "issuer",
  "title",
  "reference",
  "amount",
  "issued_at",
  "return_by",
  "warranty_until",
  "valid_until",
  "kind",
] as const;

const FIELD_LABELS: Record<(typeof FIXABLE)[number], string> = {
  issuer: "From",
  title: "What",
  reference: "Reference",
  amount: "Amount",
  issued_at: "Date",
  return_by: "Return by",
  warranty_until: "Warranty to",
  valid_until: "Valid until",
  kind: "Kind",
};

/**
 * `,`: fix a field, confirm one, or mark the whole card checked. A fix is
 * yours from then on and wins over every re-run; renaming the issuer can
 * apply to every record from the sender.
 */
export function RecordEditDialog({ record, onClose }: { record: RecordData; onClose: () => void }) {
  const current = (field: string) =>
    field === "kind"
      ? record.kind
      : ((record.fields ?? []).find((candidate) => candidate.field === field)?.value ?? "");
  const firstUnchecked = FIXABLE.find((name) => record.unchecked_fields?.includes(name));
  const initialField = firstUnchecked ?? (record.amount ? "amount" : "reference");
  const [field, setField] = useState<(typeof FIXABLE)[number]>(initialField);
  const [value, setValue] = useState(current(initialField));
  const [toSender, setToSender] = useState(false);
  const existing = (record.fields ?? []).find((candidate) => candidate.field === field);
  const changed = value.trim() !== current(field).trim() && value.trim() !== "";
  const typed = useDebouncedValue(value.trim(), 300);
  // The daemon's dry run of the fix: what the field reads as once saved,
  // or why it can't be read ("Give the currency too").
  const preview = useQuery({
    queryKey: ["record-edit-preview", record.id, field, typed, toSender],
    queryFn: () =>
      previewEdit(record, { op: "set", field, value: typed }, field === "issuer" && toSender),
    enabled: changed && typed !== "" && typed === value.trim(),
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
  const after = preview.data?.records[0]?.fields?.find((candidate) => candidate.field === field);
  function submit(event: FormEvent) {
    event.preventDefault();
    if (!changed) return;
    onClose();
    void editField(
      record,
      { op: "set", field, value: value.trim() },
      field === "issuer" && toSender,
    );
  }
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-md grid-cols-[minmax(0,1fr)]"
        data-testid="record-edit-dialog"
      >
        <DialogHeader>
          <DialogTitle>Fix a field</DialogTitle>
          <DialogDescription>What you set here is yours: mxr won't read over it.</DialogDescription>
        </DialogHeader>
        <form className="grid gap-3" onSubmit={submit}>
          <label className="grid gap-1 text-[13px]">
            <span className="font-medium">Field</span>
            <select
              value={field}
              onChange={(event) => {
                const next = FIXABLE.find((name) => name === event.target.value);
                if (!next) return;
                setField(next);
                setValue(current(next));
              }}
              className="h-9 rounded-md border border-border bg-background px-2 text-[13px]"
            >
              {FIXABLE.map((name) => (
                <option key={name} value={name}>
                  {FIELD_LABELS[name]}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1 text-[13px]">
            <span className="font-medium">{FIELD_LABELS[field]}</span>
            <Input
              autoFocus
              value={value}
              placeholder={
                field === "amount"
                  ? "£12.50"
                  : field.endsWith("_at") || field.includes("_")
                    ? "3 March 2025"
                    : ""
              }
              onChange={(event) => setValue(event.target.value)}
            />
          </label>
          {changed && after ? (
            <p data-testid="record-edit-preview" className="text-[12.5px] text-muted-foreground">
              Saves as <span className="font-mono text-foreground">{after.value}</span>, from you,
              checked.
            </p>
          ) : changed && preview.isError ? (
            <p className="text-[12.5px] text-destructive">{preview.error.message}</p>
          ) : existing ? (
            <Provenance field={existing} />
          ) : null}
          {field === "issuer" ? (
            <label className="flex items-center gap-2 text-[13px]">
              <input
                type="checkbox"
                checked={toSender}
                onChange={(event) => setToSender(event.target.checked)}
              />
              Every record from this sender, now and later
            </label>
          ) : null}
          <DialogFooter className="gap-2 sm:justify-between">
            <div className="flex gap-2">
              {existing && !existing.checked ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    onClose();
                    void editField(record, { op: "confirm", field });
                  }}
                >
                  It's right
                </Button>
              ) : null}
              {!record.checked ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    onClose();
                    void markChecked(record);
                  }}
                >
                  Mark all checked
                </Button>
              ) : null}
            </div>
            <div className="flex gap-2">
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                Cancel
              </Button>
              <Button type="submit" size="sm" disabled={!changed}>
                Save
              </Button>
            </div>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * `E`: what an export of the records in view holds, from the daemon's dry
 * run, before the CSV is written. The export reads the same rows.
 */
export function ExportDialog({
  account,
  filter,
  onClose,
}: {
  account: string | null;
  filter: RecordFilter;
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: ["records-export-preview", account ?? "all", filter],
    queryFn: () => previewExport(account, filter),
    staleTime: 0,
    gcTime: 0,
  });
  const data = preview.data;
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md" data-testid="export-dialog">
        <DialogHeader>
          <DialogTitle>Export records</DialogTitle>
          <DialogDescription>
            A CSV of the records in view, for a spreadsheet or your accountant.
          </DialogDescription>
        </DialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : preview.isError ? (
          <p className="text-[13px] text-destructive">Preview failed: {preview.error.message}</p>
        ) : data ? (
          <div className="grid gap-2 text-[13px]" data-testid="export-preview">
            <p className="text-foreground">{data.summary}</p>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-muted-foreground">
              <dt>Rows</dt>
              <dd data-testid="export-rows" className="font-mono tabular-nums text-foreground">
                {data.rows}
              </dd>
              <dt>Unchecked</dt>
              <dd data-testid="export-unchecked" className="font-mono tabular-nums text-foreground">
                {data.unchecked}
              </dd>
              <dt>No PDF</dt>
              <dd data-testid="export-missing" className="font-mono tabular-nums text-foreground">
                {data.missing_pdfs}
              </dd>
              {data.totals.map((total) => (
                <div key={total.currency} className="contents">
                  <dt>Total {total.currency}</dt>
                  <dd className="font-mono tabular-nums text-foreground">{total.display}</dd>
                </div>
              ))}
            </dl>
            {data.unchecked > 0 ? (
              <p className="text-[12.5px] text-muted-foreground">
                Rows with an unchecked amount or date say so in the CSV's checked column.
              </p>
            ) : null}
          </div>
        ) : null}
        <DialogFooter>
          <Button type="button" variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            autoFocus
            disabled={!data || data.rows === 0}
            onClick={() => {
              onClose();
              void downloadExport(account, filter);
            }}
          >
            Download CSV{data ? ` (${plural(data.rows, "row")})` : ""}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/**
 * `T` on a conversation: pass it to another mode. This phase's one choice
 * is Archive, which shows the card filing would make (the daemon's dry
 * run); Enter files exactly that.
 */
export function PassToModeDialog({
  messageId,
  subject,
  onClose,
}: {
  messageId: string;
  subject?: string;
  onClose: () => void;
}) {
  const preview = useQuery({
    queryKey: ["records-file-preview", messageId],
    queryFn: () => previewFile(messageId),
    staleTime: 0,
    gcTime: 0,
  });
  const record = preview.data?.records[0];
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="max-w-md grid-cols-[minmax(0,1fr)]"
        data-testid="pass-to-mode-dialog"
      >
        <DialogHeader>
          <DialogTitle>Pass to a mode</DialogTitle>
          <DialogDescription className="truncate">
            {subject ? `"${subject}". ` : ""}Archive: file it as a record.
          </DialogDescription>
        </DialogHeader>
        {preview.isLoading ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : preview.isError ? (
          <p className="text-[13px] text-destructive">Preview failed: {preview.error.message}</p>
        ) : record ? (
          <div
            className="grid gap-1 border-l-2 border-primary pl-3 text-[13px]"
            data-testid="file-preview"
          >
            <p className="font-medium">
              {record.kind_label}: {[record.issuer, record.title].filter(Boolean).join(", ")}
            </p>
            {record.amount ? (
              <p className="font-mono tabular-nums">{record.amount.display}</p>
            ) : null}
            {record.reference ? (
              <p className="font-mono">
                {record.reference_label} {record.reference}
              </p>
            ) : null}
            <p className="text-[12.5px] text-muted-foreground">{preview.data?.message}</p>
          </div>
        ) : null}
        <DialogFooter>
          <Button type="button" variant="ghost" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            autoFocus
            disabled={!record}
            onClick={() => {
              onClose();
              void fileMessage(messageId);
            }}
          >
            File in Archive
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
