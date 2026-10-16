/*
 * The assistant's wire: the words and marks for what the desk is doing and
 * did. The daemon writes the words (a step's two tenses, a change's title,
 * the live line's call); these pick the tense from the state and fall back
 * to the model's reason and describe.ts only for a daemon that predates
 * them. Nothing here shows a tool name unless a daemon sends nothing else.
 */

import { toolWords } from "./describe";
import type { AssistantCallState, AssistantItem, AssistantProgress } from "./api";

export type ToolCallItem = Extract<AssistantItem, { kind: "tool_call" }>;

/** After this long on one step the live line names the model, so a slow local one doesn't look stuck. */
export const SLOW_STEP_SECONDS = 20;

/** The step as one line, in the tense its state calls for. */
export function stepTitle(item: ToolCallItem): string {
  const words = item.title
    ? item.state === "running"
      ? item.title.running
      : item.title.done
    : fallbackTitle(item);
  if (item.tool === "ask_user" && item.state === "answered") {
    const answered = answeredWith(item.result);
    if (answered) return `${words}: ${answered}`;
  }
  return words;
}

/** An older daemon sends no titles: the model's reason, else the tool as words. */
function fallbackTitle(item: ToolCallItem): string {
  if (item.tool === "ask_user") {
    const answered = answeredWith(item.result);
    const reason = item.reason || "Asked you a question";
    return answered ? `${reason}: ${answered}` : reason;
  }
  return item.reason || toolWords(item.tool);
}

/** "Work, plus a note" from an ask_user result the user wrote. */
export function answeredWith(result: string | null | undefined): string | null {
  if (!result) return null;
  try {
    const parsed = JSON.parse(result) as {
      answers?: Array<{ selected?: string[]; other?: string | null }>;
    };
    const parts: string[] = [];
    for (const answer of parsed.answers ?? []) {
      parts.push(...(answer.selected ?? []));
      if (answer.other) parts.push(answer.other);
    }
    return parts.length > 0 ? parts.join(", ") : null;
  } catch {
    return null;
  }
}

export type WireTone = "running" | "done" | "failed" | "waiting" | "quiet";

/** The one glyph a step wears, and the tone it is drawn in. */
export function stepGlyph(state: AssistantCallState): { glyph: string; tone: WireTone } {
  switch (state) {
    case "running":
      return { glyph: "◌", tone: "running" };
    case "failed":
      return { glyph: "✕", tone: "failed" };
    case "waiting":
      return { glyph: "◇", tone: "waiting" };
    case "declined":
    case "cancelled":
    case "interrupted":
      return { glyph: "✕", tone: "quiet" };
    default:
      return { glyph: "✓", tone: "done" };
  }
}

export const GLYPH_WORDS: Record<AssistantCallState, string> = {
  running: "Running",
  done: "Done",
  failed: "Failed",
  waiting: "Waiting for you",
  approved: "Approved",
  auto_approved: "Ran without asking",
  declined: "Sent back",
  answered: "Answered",
  cancelled: "Stopped",
  interrupted: "Interrupted",
};

/** What the desk is doing now, the daemon's phrase when it sent one. */
export function liveLine(progress: AssistantProgress | { kind: "writing" } | null): string {
  switch (progress?.kind) {
    case "calling":
      return progress.title || progress.reason || toolWords(progress.tool);
    case "writing":
      return "Writing the answer";
    case "summarizing":
      return "Making room: summarising earlier messages";
    case "thinking":
      return progress.step > 1 ? "Deciding what to do next" : "Reading your request";
    default:
      return "Reading your request";
  }
}

/** "gemma4 is slow on this; 24s", once one step has run long enough to need it. */
export function slowNote(model: string | null | undefined, stepSeconds: number): string | null {
  if (!model || stepSeconds < SLOW_STEP_SECONDS) return null;
  return `${model} is slow on this; ${stepSeconds}s`;
}

export interface FoldSummary {
  steps: number;
  found: number | null;
  changed: number;
  waiting: number;
  failed: number;
}

/** What a finished turn's steps came to, for the one folded line. */
export function foldSummary(items: ToolCallItem[]): FoldSummary {
  let found: number | null = null;
  for (const item of items) {
    const total = item.readout?.total;
    if (total != null && item.state !== "failed") found = Math.max(found ?? 0, total);
  }
  const count = (states: AssistantCallState[]) =>
    items.filter((item) => states.includes(item.state)).length;
  return {
    steps: items.length,
    found,
    changed: count(["approved", "auto_approved"]),
    waiting: count(["waiting"]),
    failed: count(["failed"]),
  };
}

/** Changes whose effect the user can't take back from mxr. */
const IRREVERSIBLE_TOOLS = new Set([
  "mxr_send_draft",
  "mxr_got_it",
  "mxr_delete_draft",
  "mxr_sorting_rule",
  "mxr_category_rule",
  "mxr_category_delete",
  "mxr_copy_draft_to_provider",
  "mxr_sync_draft_to_provider",
  "mxr_reading_item",
]);
const SENDING_TOOLS = new Set(["mxr_send_draft", "mxr_got_it"]);

/** The warning above Apply, or null for a change that can be put back. */
export function irreversibleWarning(tool: string, args: unknown): string | null {
  const action =
    args && typeof args === "object" ? (args as Record<string, unknown>).action : undefined;
  const trashOrSpam = tool === "mxr_mutate" && (action === "trash" || action === "spam");
  if (!IRREVERSIBLE_TOOLS.has(tool) && !trashOrSpam) return null;
  return SENDING_TOOLS.has(tool) ? "Sends an email. Can't be undone." : "Can't be undone.";
}

/** Message and thread ids a mail change names, for the landing mark in the page beside. */
export function changedIds(item: ToolCallItem): string[] {
  const args = item.arguments;
  if (!args || typeof args !== "object") return [];
  const ids = (args as Record<string, unknown>).message_ids;
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === "string") : [];
}
