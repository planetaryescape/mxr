/*
 * Right-rail "Who is this?": what the archive knows about a person or
 * term (TUI `W`). Works without a language model, which only adds the
 * written summary and topics.
 */

import { useQuery } from "@tanstack/react-query";

import { fetchWhois } from "./api";
import { LlmNotice } from "./LlmNotice";
import { FactList, PageError, PageSkeleton } from "@/components/PageParts";
import { useLlmStatus } from "@/features/llm/useLlmStatus";
import { formatListDate } from "@/lib/format";

export interface WhoisPayload {
  entity: string;
  accountId?: string;
}

export function isWhoisPayload(value: unknown): value is WhoisPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { entity?: unknown }).entity === "string"
  );
}

const KIND_LABEL: Record<string, string> = {
  person: "Person",
  term: "Term",
  ambiguous: "More than one match",
  unknown: "Not found",
};

export function WhoisPanel({ entity, accountId }: WhoisPayload) {
  const llm = useLlmStatus();
  const whois = useQuery({
    queryKey: ["whois", entity, accountId ?? null],
    queryFn: () => fetchWhois(entity, accountId),
    retry: false,
  });

  if (whois.isPending) return <PageSkeleton rows={4} label={`Looking up ${entity}`} />;
  if (whois.isError)
    return (
      <div className="space-y-3">
        <PageError
          title={`Could not look up ${entity}`}
          error={whois.error}
          onRetry={() => void whois.refetch()}
        />
        {/llm|model/i.test(whois.error.message) ? (
          <LlmNotice>This lookup needs a language model.</LlmNotice>
        ) : null}
      </div>
    );

  const info = whois.data.entity;
  return (
    <div className="space-y-4 text-foreground">
      <div>
        <div className="font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground">
          {KIND_LABEL[info.kind] ?? info.kind}
        </div>
        <h3 className="mt-0.5 break-words text-[15px] font-semibold">{info.canonical_name}</h3>
      </div>
      {info.summary ? <p className="text-[13px] leading-relaxed">{info.summary}</p> : null}
      {!llm.isPending && !llm.enabled ? (
        <LlmNotice>
          This is what the archive holds. A language model adds a written summary and topics.
        </LlmNotice>
      ) : null}
      <FactList
        columns={1}
        facts={[
          ["First seen", info.first_seen_at ? formatListDate(info.first_seen_at) : "never"],
          ["Last seen", info.last_seen_at ? formatListDate(info.last_seen_at) : "never"],
        ]}
      />
      {info.topics.length > 0 ? (
        <section>
          <h4 className="mb-1.5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            Topics
          </h4>
          <ul className="flex flex-wrap gap-1">
            {info.topics.map((topic) => (
              <li key={topic} className="rounded bg-muted px-1.5 py-0.5 text-2xs">
                {topic}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {info.candidates.length > 0 ? (
        <section>
          <h4 className="mb-1.5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            Could also be
          </h4>
          <ul>
            {info.candidates.map((candidate) => (
              <li
                key={`${candidate.kind}:${candidate.value}`}
                className="border-b border-border/60 py-1.5 text-[12.5px]"
              >
                {candidate.display_name ? `${candidate.display_name} ` : ""}
                <span className="font-mono text-2xs text-muted-foreground">
                  {candidate.value} · {candidate.mention_count} mentions
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {info.citations.length > 0 ? (
        <section>
          <h4 className="mb-1.5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            From your mail
          </h4>
          <ul className="space-y-2">
            {info.citations.map((citation) => (
              <li key={`${citation.msg_id}:${citation.quote.slice(0, 24)}`}>
                <blockquote className="border-l border-border pl-2.5 text-[12.5px] text-muted-foreground">
                  {citation.quote}
                </blockquote>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
