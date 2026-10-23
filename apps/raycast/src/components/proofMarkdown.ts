import { proofRuns, runsText } from "../../../web/src/features/compose/assist/proof";
import type { ComposeAssistProposal } from "../bridge/types";
import { escapeLine } from "./markdown";

/** Show the daemon's word changes without interpreting proposal text as markdown. */
export function proposalMarkdown(proposal: ComposeAssistProposal): string {
  const runs = proofRuns(proposal.changes, proposal.gaps);
  if (runsText(runs) !== proposal.text) {
    return escapeText(proposal.text);
  }

  return runs
    .map((run) => {
      if (run.gap !== null) return inlineCode(run.text);
      const text = escapeText(run.text);
      if (run.kind === "added") return `**${text}**`;
      if (run.kind === "removed") return `~~${text}~~`;
      return text;
    })
    .join("");
}

function escapeText(text: string): string {
  return text.split("\n").map(escapeLine).join("\n");
}

function inlineCode(text: string): string {
  const ticks = text.match(/`+/g) ?? [];
  const fence = "`".repeat(Math.max(1, ...ticks.map((value) => value.length + 1)));
  const padding = /^\s|\s$/.test(text) ? " " : "";
  return `${fence}${padding}${text.replace(/\n/g, " ")}${padding}${fence}`;
}
