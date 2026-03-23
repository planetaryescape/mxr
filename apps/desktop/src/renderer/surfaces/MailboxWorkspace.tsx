import type {
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  ThreadResponse,
  UtilityRailPayload,
} from "../../shared/types";
import { Paperclip, Star } from "lucide-react";
import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import { ReaderPane } from "./ReaderPane";
import type { FlattenedEntry } from "./types";

export function MailboxWorkspace(props: {
  mailbox: MailboxPayload;
  rows: FlattenedEntry[];
  mailListMode: "threads" | "messages";
  selectedThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  onSelect: (threadId: string) => void;
  onOpen: () => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
  utilityRail: UtilityRailPayload;
}) {
  const showReader = props.layoutMode !== "twoPane";
  const listRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!props.selectedThreadId) {
      return;
    }
    const selectedRow = listRef.current?.querySelector<HTMLElement>(
      `[data-thread-id="${props.selectedThreadId}"]`,
    );
    if (selectedRow && typeof selectedRow.scrollIntoView === "function") {
      selectedRow.scrollIntoView({ block: "nearest" });
    }
  }, [props.rows, props.selectedThreadId]);

  return (
    <div
      className={cn(
        "grid h-full min-h-0 grid-cols-1",
        props.layoutMode === "threePane" ? "xl:grid-cols-[minmax(20rem,0.84fr)_minmax(0,1fr)]" : "",
      )}
    >
      <section
        ref={listRef}
        className={cn(
          "subtle-scrollbar min-h-0 overflow-y-auto border-r border-outline bg-panel px-2.5 py-2.5",
          props.layoutMode === "fullScreen" ? "hidden" : "",
        )}
      >
        <div className="flex items-end justify-between gap-2 border-b border-outline pb-2">
          <div>
            <p className="mono-meta">Mailbox</p>
            <h1 className="mt-0.5 text-balance text-[1.15rem] font-semibold leading-none text-foreground">
              {props.mailListMode === "threads" ? "Threads" : "Messages"}
            </h1>
            <p className="mt-0.5 text-[11px] text-foreground-muted">
              Active lens <span className="font-medium text-foreground">{props.mailbox.lensLabel}</span>
            </p>
          </div>
          <div className="text-right tabular-nums">
            <p className="font-mono text-[9px] text-foreground-subtle">
              {props.mailbox.counts.unread} unread
            </p>
            <p className="mt-0.5 font-mono text-[9px] text-foreground-subtle">
              {props.mailbox.counts.total} total
            </p>
          </div>
        </div>

        <div className="mt-1.5 space-y-px">
          {props.rows.map((entry) =>
            entry.kind === "header" ? (
              <div
                key={entry.id}
                className="border-b border-outline/60 pb-1 pt-1.5 text-[9px] uppercase text-foreground-subtle first:pt-0"
              >
                {entry.label}
              </div>
            ) : (
              <button
                key={entry.id}
                data-thread-id={entry.row.thread_id}
                className={cn(
                  "w-full border px-2 py-1.5 text-left transition-colors",
                  props.selectedThreadId === entry.row.thread_id
                    ? "border-accent/40 bg-panel-elevated"
                    : props.selectedMessageIds.has(entry.row.id)
                      ? "border-success/30 bg-success/8"
                      : "border-transparent bg-transparent hover:bg-panel-elevated/55",
                )}
                onClick={() => props.onSelect(entry.row.thread_id)}
                onDoubleClick={props.onOpen}
              >
                <div className="flex items-start gap-1.5">
                  <span
                    className={cn(
                      "mt-[3px] size-1.5 shrink-0 rounded-full",
                      entry.row.unread ? "bg-accent" : "bg-outline",
                    )}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0">
                        <div className="flex items-center gap-1">
                          <span
                            className={cn(
                              "truncate text-[10px]",
                              entry.row.unread
                                ? "font-semibold text-foreground"
                                : "font-medium text-foreground-muted",
                            )}
                          >
                            {entry.row.sender}
                          </span>
                          {entry.row.starred ? (
                            <Star className="size-3.5 shrink-0 fill-warning text-warning" />
                          ) : null}
                          {entry.row.has_attachments ? (
                            <Paperclip className="size-3.5 shrink-0 text-foreground-subtle" />
                          ) : null}
                        </div>
                        <p
                          className={cn(
                            "mt-0.5 line-clamp-1 text-[11px] leading-4",
                            entry.row.unread
                              ? "font-semibold text-foreground"
                              : "font-medium text-foreground",
                          )}
                        >
                          {entry.row.subject}
                        </p>
                      </div>
                      <div className="flex shrink-0 flex-col items-end gap-1">
                        {props.pendingMessageIds.has(entry.row.id) ? (
                          <span className="border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[9px] text-accent">
                            Syncing
                          </span>
                        ) : null}
                        <span className="font-mono text-[9px] tabular-nums text-foreground-subtle">
                          {entry.row.date_label}
                        </span>
                      </div>
                    </div>
                    <p className="mt-0.5 line-clamp-1 text-[10px] leading-3.5 text-foreground-subtle text-pretty">
                      {entry.row.snippet}
                    </p>
                  </div>
                </div>
              </button>
            ),
          )}
        </div>
      </section>

      {showReader ? (
        <ReaderPane
          className={cn(
            props.layoutMode === "fullScreen" ? "min-h-0 flex" : "hidden min-h-0 xl:flex",
            props.layoutMode === "fullScreen" ? "xl:col-span-2" : "",
          )}
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
        />
      ) : null}

      {showReader && props.layoutMode !== "fullScreen" ? (
        <ReaderPane
          className="fixed inset-y-12 right-0 z-10 flex w-[min(100vw-4rem,32rem)] border-l border-outline bg-panel xl:hidden"
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
        />
      ) : null}
    </div>
  );
}
