import { find as findLinks } from "linkifyjs";
import { MoreHorizontal } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";

import { plural } from "@/lib/format";

import { findQuote } from "./context/askQuote";
import { normalizeSegments, splitMessageText, type Segment } from "./textSegments";

/**
 * A plain-text or reader-mode body: the new writing in full, quoted
 * history and signatures folded behind a control that says how much is
 * hidden. `showQuotes` / `showSignature` come from the reader toggles
 * (Q, S); each fold can also be opened on its own.
 */
export function MessageText({
  text,
  showQuotes,
  showSignature,
  plain = false,
  highlight,
}: {
  text: string;
  showQuotes: boolean;
  showSignature: boolean;
  /** The sender's exact text in monospace: folds, but no reflow or mark stripping. */
  plain?: boolean;
  /** The ask's quote, marked where it appears. */
  highlight?: string;
}) {
  const segments = useMemo(() => normalizeSegments(splitMessageText(text)), [text]);
  return (
    <div
      className={
        plain
          ? "max-w-[var(--reading-measure)] font-mono text-[13px] leading-6 text-foreground"
          : "max-w-[var(--reading-measure)] text-[14.5px] leading-[1.65] text-foreground"
      }
    >
      {segments.map((segment, index) => (
        <SegmentView
          // Segments are derived from immutable text; position is identity.
          // oxlint-disable-next-line react/no-array-index-key
          key={index}
          segment={segment}
          plain={plain}
          highlight={highlight}
          forceOpen={
            segment.kind === "quote"
              ? showQuotes
              : segment.kind === "signature"
                ? showSignature
                : true
          }
        />
      ))}
    </div>
  );
}

function SegmentView({
  segment,
  forceOpen,
  plain,
  highlight,
}: {
  segment: Segment;
  forceOpen: boolean;
  plain: boolean;
  highlight?: string;
}) {
  const [open, setOpen] = useState(false);
  if (segment.kind === "text") return <Paragraphs text={segment.text} highlight={highlight} />;
  const expanded = forceOpen || open;
  const label = segment.kind === "quote" ? `${plural(segment.lines, "quoted line")}` : "Signature";
  if (!expanded) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={
          segment.kind === "quote"
            ? "Show quoted text (Q shows all)"
            : "Show signature (S shows all)"
        }
        className="my-2 inline-flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-0.5 font-mono text-2xs text-muted-foreground hover:border-border-strong hover:text-foreground"
      >
        <MoreHorizontal className="size-3.5" />
        {label}
      </button>
    );
  }
  return (
    <div
      className={
        segment.kind === "quote"
          ? "my-2 border-l-2 border-border-strong pl-3 text-muted-foreground"
          : "mt-3 text-[13px] text-muted-foreground"
      }
    >
      <Paragraphs text={plain ? segment.text : stripQuoteMarks(segment.text)} />
      {!forceOpen ? (
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="mt-1 font-mono text-2xs text-muted-foreground hover:text-foreground"
        >
          Hide {segment.kind === "quote" ? "quoted text" : "signature"}
        </button>
      ) : null}
    </div>
  );
}

/** "> " marks carry no meaning once the block is visually indented. */
function stripQuoteMarks(text: string): string {
  return text
    .split("\n")
    .map((line) => line.replace(/^\s*>\s?/, ""))
    .join("\n");
}

function Paragraphs({ text, highlight }: { text: string; highlight?: string }) {
  const { tidy, range } = useMemo(() => {
    const cleaned = text.replace(/\n{3,}/g, "\n\n").replace(/^\n+|\n+$/g, "");
    return { tidy: cleaned, range: highlight ? findQuote(cleaned, highlight) : null };
  }, [text, highlight]);
  return (
    <div className="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
      {range ? (
        <>
          <Linkified text={tidy.slice(0, range.start)} />
          <mark
            data-ask-quote=""
            className="rounded-[2px] border-b border-[var(--ask-mark-rule)] bg-[var(--ask-mark)] px-0.5 text-foreground [box-decoration-break:clone]"
          >
            <Linkified text={tidy.slice(range.start, range.end)} />
          </mark>
          <Linkified text={tidy.slice(range.end)} />
        </>
      ) : (
        <Linkified text={tidy} />
      )}
    </div>
  );
}

function Linkified({ text }: { text: string }) {
  const nodes = useMemo(() => {
    const links = findLinks(text, { defaultProtocol: "https" });
    if (links.length === 0) return [text];
    const out: ReactNode[] = [];
    let cursor = 0;
    for (const link of links) {
      if (link.start > cursor) out.push(text.slice(cursor, link.start));
      out.push(
        <a
          key={`${link.start}-${link.end}`}
          href={link.href}
          target="_blank"
          rel="noopener noreferrer"
          className="text-primary underline decoration-primary/40 underline-offset-2 hover:decoration-primary"
        >
          {text.slice(link.start, link.end)}
        </a>,
      );
      cursor = link.end;
    }
    if (cursor < text.length) out.push(text.slice(cursor));
    return out;
  }, [text]);
  return <>{nodes}</>;
}
