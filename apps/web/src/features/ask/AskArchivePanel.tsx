/*
 * Right-rail "Ask your archive": a question in, an answer with the
 * messages it came from out. Without a language model the daemon returns
 * the retrieved evidence instead of a written answer, and this says so.
 */

import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { askArchive } from "./api";
import { LlmNotice } from "./LlmNotice";
import { PageError, PageSkeleton } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { useLlmStatus } from "@/features/llm/useLlmStatus";
import { formatWhen, plural } from "@/lib/format";

export function AskArchivePanel() {
  const navigate = useNavigate();
  const llm = useLlmStatus();
  const [question, setQuestion] = useState("");
  const ask = useMutation({ mutationFn: (text: string) => askArchive(text) });
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Opened from the command palette, whose dialog hands focus back to its
  // trigger as it closes, after this mounts. Take focus again once that
  // settles so typing lands in the question, not on the page's keys.
  useEffect(() => {
    inputRef.current?.focus();
    const timer = setTimeout(() => {
      if (document.activeElement !== inputRef.current) inputRef.current?.focus();
    }, 250);
    return () => clearTimeout(timer);
  }, []);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const text = question.trim();
    if (text && !ask.isPending) ask.mutate(text);
  };
  const answer = ask.data?.answer;

  return (
    <div className="space-y-4 text-foreground">
      <form onSubmit={submit} className="space-y-2">
        <Textarea
          ref={inputRef}
          aria-label="Question for your archive"
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={(event) => {
            // Enter asks; Shift+Enter adds a line.
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          placeholder="When did we agree the launch date?"
          className="min-h-20 text-[13px]"
        />
        <Button type="submit" size="sm" disabled={!question.trim() || ask.isPending}>
          <Search className="size-3" />
          {ask.isPending ? "Searching…" : "Ask"}
        </Button>
      </form>
      {!llm.isPending && !llm.enabled ? (
        <LlmNotice>
          No language model is set up, so you get the matching messages rather than a written
          answer.
        </LlmNotice>
      ) : null}
      {ask.isPending ? (
        <PageSkeleton rows={3} label="Searching your archive" />
      ) : ask.isError ? (
        <PageError
          title="The archive could not answer"
          error={ask.error}
          onRetry={() => ask.mutate(ask.variables)}
        />
      ) : answer ? (
        <div className="space-y-4">
          <p className="whitespace-pre-wrap text-[13px] leading-relaxed">{answer.text}</p>
          <p className="font-mono text-2xs text-muted-foreground">
            {plural(answer.retrieval.candidate_count, "message")} considered,{" "}
            {answer.retrieval.executed_mode} search
          </p>
          {answer.citations.length > 0 ? (
            <section>
              <h4 className="mb-1.5 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
                Sources
              </h4>
              <ul>
                {answer.citations.map((citation) => (
                  <li key={citation.message_id} className="border-b border-border/60">
                    <button
                      type="button"
                      onClick={() =>
                        void navigate({
                          to: "/m/$mailbox/$threadId",
                          params: { mailbox: "archive", threadId: citation.thread_id },
                        })
                      }
                      className="block w-full px-1 py-2 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <div className="flex items-baseline gap-2">
                        <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium">
                          {citation.subject.trim() || "(no subject)"}
                        </span>
                        <span className="shrink-0 font-mono text-2xs text-muted-foreground">
                          {formatWhen(citation.date)}
                        </span>
                      </div>
                      <p className="mt-0.5 line-clamp-3 text-2xs text-muted-foreground">
                        {citation.quote}
                      </p>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
