import type { ReplyDraft } from "../bridge/compose";
import type { SafetyReport } from "../bridge/types";

export function proposalStillOwnsReply(input: {
  closed: boolean;
  currentDraft: ReplyDraft | undefined;
  owner: ReplyDraft;
  currentWorkspace: string;
  workspace: string;
  currentBody: string;
  baseline: string;
}): boolean {
  return (
    !input.closed &&
    input.currentDraft === input.owner &&
    input.currentWorkspace === input.workspace &&
    input.currentBody === input.baseline
  );
}

export function shouldDiscardReply(handedOver: boolean, proofOpen: boolean): boolean {
  return !handedOver && !proofOpen;
}

export function requestIsCurrent(request: number, latest: number): boolean {
  return request === latest;
}

export function hasUnfilledGap(report: SafetyReport): boolean {
  return report.issues.some((issue) => issue.code === "unfilled_gap");
}
