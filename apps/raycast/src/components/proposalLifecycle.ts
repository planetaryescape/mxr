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

/** Owns one in-flight UI operation; an old completion cannot release a new one. */
export class OwnedRequestGate {
  private generation = 0;
  private active?: number;

  get isRunning(): boolean {
    return this.active !== undefined;
  }

  begin(): number | undefined {
    if (this.isRunning) return undefined;
    const request = ++this.generation;
    this.active = request;
    return request;
  }

  isCurrent(request: number): boolean {
    return this.active === request;
  }

  finish(request: number): void {
    if (this.active === request) this.active = undefined;
  }

  cancel(): void {
    this.generation += 1;
    this.active = undefined;
  }
}

export function hasUnfilledGap(report: SafetyReport): boolean {
  return report.issues.some((issue) => issue.code === "unfilled_gap");
}
