import { useEffect, useState } from "react";
import {
  ARCHIVE_JOURNAL_CHANGED,
  needsRecovery,
  queryArchive,
  retainedArchives,
  submitArchive,
  type ArchiveOperation,
} from "./archiveRecovery";

export function ArchiveRecoveryPanel() {
  const [operations, setOperations] = useState<ArchiveOperation[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => {
    const refresh = () => {
      try {
        // A finished Archive is journalled too; the panel is for the ones that aren't.
        setOperations(retainedArchives().filter(needsRecovery));
      } catch (caught) {
        setError(String(caught));
      }
    };
    refresh();
    window.addEventListener(ARCHIVE_JOURNAL_CHANGED, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(ARCHIVE_JOURNAL_CHANGED, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);
  async function recover(operation: ArchiveOperation, submit: boolean) {
    setBusy(operation.identity.operation_id);
    setError(null);
    try {
      await (submit ? submitArchive(operation) : queryArchive(operation));
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(null);
    }
  }
  if (operations.length === 0 && !error) return null;
  return (
    <aside
      className="fixed bottom-10 right-4 z-40 max-h-[70vh] max-w-xl overflow-auto rounded-sm border border-border bg-background p-3 shadow-lg"
      aria-label="Archive recovery"
    >
      <button type="button" onClick={() => setOpen(!open)}>
        Archive recovery ({operations.length})
      </button>
      {error && <p role="alert">{error}</p>}
      {open &&
        operations.map((operation) => (
          <section
            key={operation.identity.operation_id}
            className="mt-3 border-t border-border pt-2"
          >
            <p className="break-all font-mono text-xs">{operation.identity.operation_id}</p>
            <p>
              {operation.accepted
                ? `Last known: Archive accepted${operation.running ? " and running" : ""}.`
                : "Last known: review only; submission may have been accepted."}
            </p>
            <p>Query outcome before deciding whether to submit or retry.</p>
            <ul>
              {operation.items.map((item) => (
                <li key={item.message_id} className="break-all text-xs">
                  {item.message_id}: {item.state}
                  {item.reason ? ` — ${item.reason}` : ""}
                </li>
              ))}
            </ul>
            <button
              type="button"
              disabled={busy !== null}
              onClick={() => void recover(operation, false)}
            >
              Query outcome
            </button>
            {!operation.accepted && (
              <button
                type="button"
                disabled={busy !== null}
                className="ml-3"
                onClick={() => void recover(operation, true)}
              >
                Submit reviewed selection
              </button>
            )}
            {operation.accepted && operation.items.some((item) => item.state === "pending") && (
              <button
                type="button"
                disabled={busy !== null}
                className="ml-3"
                onClick={() => void recover(operation, true)}
              >
                Resume undispatched items
              </button>
            )}
            {operation.items.some((item) => item.state === "unknown") && (
              <p>Provider acceptance is uncertain. Query again; do not repeat this effect.</p>
            )}
          </section>
        ))}
    </aside>
  );
}
