import { proofRuns, runsText } from "../../../web/src/features/compose/assist/proof";
import type { ComposeAssistProposal } from "../bridge/types";
import { escapeLine } from "./markdown";

/** Show the daemon's word changes without interpreting proposal text as markdown. */
export function proposalMarkdown(proposal: ComposeAssistProposal): string {
  const runs = proofRuns(proposal.changes, proposal.gaps);
  if (runsText(runs) !== proposal.text) {
    return escapeProofText(proposal.text);
  }

  return coalesceRuns(runs)
    .map((run) => {
      if (run.gap !== null) return inlineCode(run.text);
      return formatRun(run.kind, run.text);
    })
    .join("");
}

function coalesceRuns(runs: ReturnType<typeof proofRuns>): ReturnType<typeof proofRuns> {
  return runs.reduce<typeof runs>((merged, run) => {
    const previous = merged.at(-1);
    if (
      previous &&
      ((run.gap !== null && run.gap === previous.gap) ||
        (run.gap === null && previous.gap === null && run.kind === previous.kind))
    ) {
      previous.text += run.text;
    } else {
      merged.push({ ...run });
    }
    return merged;
  }, []);
}

function formatRun(kind: "kept" | "added" | "removed", text: string): string {
  const escaped = escapeProofText(text);
  if (kind === "kept") return escaped;
  const marker = kind === "added" ? "**" : "~~";
  // CommonMark emphasis cannot contain paragraph breaks. Keep leading and
  // trailing spaces outside the markers so adjacent runs remain parseable.
  return escaped
    .split(/(\n)/)
    .map((line) => {
      if (line === "\n") return line;
      const match = /^(\s*)([\s\S]*?)(\s*)$/.exec(line);
      if (!match) return line;
      const [, leading, content, trailing] = match;
      return content ? `${leading}${marker}${content}${marker}${trailing}` : line;
    })
    .join("");
}

export function escapeProofText(text: string): string {
  return text.split("\n").map(escapeLine).join("\n");
}

function inlineCode(text: string): string {
  const ticks = text.match(/`+/g) ?? [];
  const fence = "`".repeat(Math.max(1, ...ticks.map((value) => value.length + 1)));
  const padding = /^\s|\s$/.test(text) ? " " : "";
  return `${fence}${padding}${text.replace(/\n/g, " ")}${padding}${fence}`;
}
