import type { components } from "@/api/generated";
import { firstName } from "@/features/thread/context/contextFormat";

export type DraftProvenance = components["schemas"]["DraftProvenanceData"];
export type DraftSource = components["schemas"]["DraftSourceData"];

export function shortPerson(source: DraftSource): string {
  return firstName({ display_name: source.person_name ?? null, email: source.person });
}

export function draftProvenanceDetail(provenance: DraftProvenance): string {
  const parts: string[] = [];
  const examples = provenance.voice_examples ?? [];
  const first = examples[0];
  if (first) {
    const samePerson = examples.every(
      (source) => source.person.toLowerCase() === first.person.toLowerCase(),
    );
    parts.push(
      samePerson && first.person
        ? `used ${examples.length} of your emails to ${shortPerson(first)}`
        : `used ${examples.length} of your emails`,
    );
  }
  parts.push(provenance.history_used ? "history used" : "history not used");
  const rewrite = provenance.rewrite;
  if (rewrite) {
    const by = `${rewrite.locality === "local" ? "local" : "cloud"} model ${rewrite.model}`;
    parts.push(
      rewrite.outcome === "rejected"
        ? `rewrite attempted by ${by}, not used`
        : rewrite.outcome === "skipped"
          ? `rewrite by ${by} skipped to keep your history local`
          : rewrite.rejected_by
            ? `rewritten by ${by}, a later pass by ${rewrite.rejected_by} not used`
            : `rewritten by ${by}`,
    );
  }
  return parts.join(" · ");
}

export function draftSourceLabel(source: DraftSource, kind: "voice" | "conversation"): string {
  if (kind === "voice") return `Your email to ${shortPerson(source)}`;
  return source.from_me ? "You" : shortPerson(source);
}

export function draftSourceRoute(source: DraftSource) {
  return {
    to: "/m/$mailbox/$threadId",
    params: { mailbox: source.from_me ? "sent" : "archive", threadId: source.thread_id },
    search: { message: source.message_id },
  } as const;
}
