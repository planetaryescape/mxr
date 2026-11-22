import { toast } from "sonner";

import { claimUndo, offerUndo } from "@/features/mail-actions/mailUndo";
import type { Verb } from "@/features/mail-actions/verbFeedback";
import { openAttachment } from "@/features/mailbox/api";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  dismissRecords,
  exportRecords,
  fileRecord,
  RECORDS_KEY,
  setRecordField,
  type RecordChange,
  type RecordData,
  type RecordEdit,
  type RecordFilter,
} from "./api";
import { amountCopy, referenceCopy } from "./ledger";

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export async function refreshRecords(): Promise<void> {
  const qc = getActiveQueryClient();
  if (!qc) return;
  await Promise.all([
    qc.invalidateQueries({ queryKey: RECORDS_KEY }),
    qc.invalidateQueries({ queryKey: ["mode-guide", "archive"] }),
  ]).catch(() => undefined);
}

export async function copyText(text: string | null, what: string): Promise<void> {
  if (!text) {
    toast.info(`This record has no ${what}`);
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    toast.success(`${text} copied`);
  } catch (error) {
    toast.error(`Couldn't copy the ${what}`, { description: errorText(error) });
  }
}

export const copyReference = (record: RecordData) => copyText(referenceCopy(record), "reference");
export const copyAmount = (record: RecordData) => copyText(amountCopy(record), "amount");

export async function openDocument(record: RecordData): Promise<void> {
  const pdf = record.pdf;
  if (!pdf) {
    toast.info("No PDF on this record. Press o to open the email");
    return;
  }
  if (refuseWhileDaemonDown("open the document")) return;
  try {
    await openAttachment({ messageId: pdf.message_id, attachmentId: pdf.attachment_id });
    toast.success(`Opened ${pdf.filename}`);
  } catch (error) {
    toast.error("Couldn't open the document", { description: errorText(error) });
  }
}

function reverseOf(change: RecordChange): (() => Promise<boolean>) | null {
  const undo = change.undo;
  if (!undo) return null;
  const run = async (request: () => Promise<unknown>) => {
    if (refuseWhileDaemonDown("undo")) return false;
    try {
      await request();
      toast.success("Undone");
      return true;
    } catch (error) {
      toast.error("Undo failed", { description: errorText(error) });
      return false;
    } finally {
      await refreshRecords();
    }
  };
  switch (undo.kind) {
    case "restore":
      return () => run(() => dismissRecords(undo.record_ids, true));
    case "dismiss":
      return () => run(() => dismissRecords(undo.record_ids, false));
    case "clear_fields":
      return () =>
        run(() =>
          Promise.all(
            undo.record_ids.flatMap((id) =>
              (undo.fields ?? []).map((field) => setRecordField(id, { op: "clear", field })),
            ),
          ),
        );
    default:
      return null;
  }
}

async function withUndo(
  verb: Verb,
  run: () => Promise<RecordChange>,
  failure: string,
): Promise<RecordChange | null> {
  const claim = claimUndo();
  try {
    const change = await run();
    claim.settle(
      offerUndo(
        verb,
        change.message,
        `record-${change.action}-${change.records.map((record) => record.id).join(",")}`,
        reverseOf(change),
        claim.run,
      ),
    );
    return change;
  } catch (error) {
    claim.settle(null);
    toast.error(failure, { description: errorText(error) });
    return null;
  } finally {
    await refreshRecords();
  }
}

export function markNotRecord(record: RecordData) {
  if (refuseWhileDaemonDown("change it")) return;
  return withUndo(
    "record-dismiss",
    () => dismissRecords([record.id]),
    "Couldn't take it out of Archive",
  );
}

export function markChecked(record: RecordData) {
  if (refuseWhileDaemonDown("change it")) return;
  if (record.checked) {
    toast.info("Already checked");
    return;
  }
  return withUndo(
    "record-check",
    () => setRecordField(record.id, { op: "confirm_all" }),
    "Couldn't mark it checked",
  );
}

export function editField(record: RecordData, edit: RecordEdit, applyToSender = false) {
  if (refuseWhileDaemonDown("change it")) return;
  return withUndo(
    "record-fix",
    () => setRecordField(record.id, edit, { applyToSender }),
    "Couldn't change the record",
  );
}

export function previewEdit(record: RecordData, edit: RecordEdit, applyToSender = false) {
  return setRecordField(record.id, edit, { dryRun: true, applyToSender });
}

export function previewFile(messageId: string) {
  return fileRecord(messageId, true);
}

export function fileMessage(messageId: string) {
  if (refuseWhileDaemonDown("file it")) return;
  return withUndo("record-file", () => fileRecord(messageId, false), "Couldn't file it in Archive");
}

export function previewExport(account: string | null, filter: RecordFilter) {
  return exportRecords(account, filter, true);
}

export async function downloadExport(account: string | null, filter: RecordFilter): Promise<void> {
  if (refuseWhileDaemonDown("export")) return;
  try {
    const answer = await exportRecords(account, filter, false);
    const csv = answer.csv ?? "";
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `mxr-records${filter.year != null ? `-${filter.year}` : ""}.csv`;
    document.body.append(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    toast.success(`Exported ${answer.summary}`);
  } catch (error) {
    toast.error("Couldn't export the records", { description: errorText(error) });
  }
}
