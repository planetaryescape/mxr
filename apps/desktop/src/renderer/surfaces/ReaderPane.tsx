import type { ReaderMode, ThreadResponse } from "../../shared/types";
import { cn } from "../lib/cn";
import { renderReaderBody, renderReaderParagraphs } from "./formatters";

export function ReaderPane(props: {
  className?: string;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
}) {
  const htmlBody = props.thread?.bodies.find((body) => body.text_html)?.text_html ?? null;
  const plainBody = props.thread?.bodies.find((body) => body.text_plain)?.text_plain ?? null;
  const rawBody =
    props.thread?.bodies.find((body) => body.raw_source)?.raw_source ??
    htmlBody ??
    plainBody ??
    null;

  return (
    <section className={cn("min-h-0 flex-1 flex-col bg-panel", props.className)}>
      <div className="border-b border-outline px-3 py-2">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 max-w-4xl">
            <div className="flex items-center gap-2">
              <p className="mono-meta">Thread</p>
              {props.thread?.messages[0] ? (
                <span className="font-mono text-[9px] tabular-nums text-foreground-subtle">
                  {props.thread.messages.length} msgs
                </span>
              ) : null}
            </div>
            <h2 className="mt-0.5 text-balance text-[1.1rem] font-semibold leading-tight text-foreground">
              {props.thread?.thread.subject ?? "Select a thread"}
            </h2>
            {props.thread?.messages[0] ? (
              <div className="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[10px] text-foreground-muted">
                <span className="font-medium text-foreground">{props.thread.messages[0].sender}</span>
                {props.thread.messages[0].sender_detail ? (
                  <span>{props.thread.messages[0].sender_detail}</span>
                ) : null}
                <span className="font-mono text-[9px] tabular-nums text-foreground-subtle">
                  {props.thread.messages[0].date_label}
                </span>
              </div>
            ) : null}
          </div>
          <div className="flex shrink-0 gap-1">
            <button
              className="h-6 border border-outline bg-canvas-elevated px-2 text-[10px] uppercase text-foreground-muted transition-colors hover:border-outline-strong hover:bg-panel-elevated hover:text-foreground"
              onClick={props.onArchive}
            >
              Archive
            </button>
            <button
              className="h-6 border border-outline bg-canvas-elevated px-2 text-[10px] uppercase text-foreground-muted transition-colors hover:border-outline-strong hover:bg-panel-elevated hover:text-foreground"
              onClick={props.onCloseReader}
            >
              Close
            </button>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1 border-b border-outline px-3 py-1">
        {(["auto", "reader", "html", "raw"] as ReaderMode[]).map((mode) => (
          <button
            key={mode}
            className={cn(
              "h-5 border px-1.5 text-[9px] uppercase transition-colors",
              props.readerMode === mode
                ? "border-accent/35 bg-accent/10 text-foreground"
                : "border-outline bg-canvas-elevated text-foreground-subtle hover:border-outline-strong hover:text-foreground-muted",
            )}
            onClick={() => props.setReaderMode(mode)}
          >
            {mode}
          </button>
        ))}
      </div>

      <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto bg-panel px-3 py-2">
        {!props.thread ? (
          <EmptyReaderState />
        ) : props.readerMode === "html" && htmlBody ? (
          <iframe
            className="h-[calc(100vh-12rem)] w-full border border-outline bg-white"
            srcDoc={htmlBody}
            title="HTML message"
          />
        ) : props.readerMode === "raw" ? (
          <pre className="mx-auto max-w-4xl whitespace-pre-wrap text-[11px] leading-4.5 text-foreground-muted">
            {rawBody ?? "No raw body available"}
          </pre>
        ) : (
          <article className="mx-auto max-w-4xl">
            <div className="space-y-2.5">
              {renderReaderParagraphs(
                renderReaderBody(
                  plainBody ?? props.thread.thread.snippet ?? "No readable body",
                  props.signatureExpanded,
                ),
              ).map((paragraph, index) => (
                <p
                  key={`${index}-${paragraph.slice(0, 24)}`}
                  className="whitespace-pre-wrap text-pretty text-[12px] leading-5 text-foreground-muted"
                >
                  {paragraph}
                </p>
              ))}
            </div>
          </article>
        )}
      </div>
    </section>
  );
}

function EmptyReaderState() {
  return (
    <section className="flex min-h-0 items-center justify-center bg-panel-muted px-4 py-5">
      <div className="max-w-md text-center">
        <p className="mono-meta">Reader</p>
        <h2 className="mt-1 text-balance text-[1.05rem] font-semibold leading-tight text-foreground">
          Open a thread to read
        </h2>
        <p className="mt-1.5 text-pretty text-[11px] leading-4.5 text-foreground-muted">
          Two-pane by default. Open a message to move into three-pane and keep the list visible.
        </p>
      </div>
    </section>
  );
}
