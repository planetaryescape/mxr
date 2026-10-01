import { Link } from "@tanstack/react-router";
import { ArrowUpRight, ChevronDown } from "lucide-react";
import { useState } from "react";

import { formatWhen } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  draftProvenanceLine,
  draftSourceLabel,
  draftSourceRoute,
  type DraftProvenance,
  type DraftSource,
} from "./draftProvenanceFormat";

/**
 * The quiet line under an AI draft saying where it came from, with the
 * messages it was written from one click away.
 */
export function DraftProvenanceLine({
  provenance,
  className,
}: {
  provenance: DraftProvenance | null | undefined;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  if (!provenance) return null;
  const voice = provenance.voice_examples ?? [];
  const conversation = provenance.conversation ?? [];
  const count = voice.length + conversation.length;
  return (
    <div data-testid="draft-provenance" className={cn("text-2xs text-muted-foreground", className)}>
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span data-testid="draft-provenance-line">{draftProvenanceLine(provenance)}</span>
        {count > 0 ? (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen((value) => !value)}
            className="inline-flex items-center gap-0.5 underline decoration-border-strong decoration-1 underline-offset-2 hover:text-foreground"
          >
            {open ? "Hide sources" : `Sources (${count})`}
            <ChevronDown
              aria-hidden
              className={cn("size-3 transition-transform duration-fast", open && "rotate-180")}
            />
          </button>
        ) : null}
      </div>
      {open ? (
        <div className="mt-1.5 space-y-1.5">
          <SourceList title="Your emails it matched the voice of" sources={voice} kind="voice" />
          <SourceList
            title="Messages it read from this conversation"
            sources={conversation}
            kind="conversation"
          />
        </div>
      ) : null}
    </div>
  );
}

function SourceList({
  title,
  sources,
  kind,
}: {
  title: string;
  sources: DraftSource[];
  kind: "voice" | "conversation";
}) {
  if (sources.length === 0) return null;
  return (
    <section aria-label={title}>
      <h4 className="font-medium text-foreground/80">{title}</h4>
      <ul className="mt-0.5 space-y-0.5">
        {sources.map((source) => (
          <li key={source.message_id}>
            {/* A new tab: opening a source in place would move or cover the
                draft being judged, and the list the user came from. */}
            <Link
              {...draftSourceRoute(source)}
              target="_blank"
              rel="noopener"
              title="Opens in a new tab"
              className="inline-flex items-center gap-2 hover:text-foreground hover:underline"
            >
              <span>{draftSourceLabel(source, kind)}</span>
              <time dateTime={source.date} className="font-mono tabular-nums">
                {formatWhen(source.date)}
              </time>
              <ArrowUpRight aria-hidden className="size-3" />
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
