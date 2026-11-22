import { toolWords } from "./describe";
import type { AssistantCallState, AssistantItem, AssistantProgress } from "./api";

export type ToolCallItem = Extract<AssistantItem, { kind: "tool_call" }>;

export const SLOW_STEP_SECONDS = 20;

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

function fallbackTitle(item: ToolCallItem): string {
  if (item.tool === "ask_user") {
    const answered = answeredWith(item.result);
    const reason = item.reason || "Asked you a question";
    return answered ? `${reason}: ${answered}` : reason;
  }
  return item.reason || toolWords(item.tool);
}

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

export function irreversibleWarning(tool: string, args: unknown): string | null {
  const action =
    args && typeof args === "object" ? (args as Record<string, unknown>).action : undefined;
  const trashOrSpam = tool === "mxr_mutate" && (action === "trash" || action === "spam");
  if (!IRREVERSIBLE_TOOLS.has(tool) && !trashOrSpam) return null;
  return SENDING_TOOLS.has(tool) ? "Sends an email. Can't be undone." : "Can't be undone.";
}

export function changedIds(item: ToolCallItem): string[] {
  const args = item.arguments;
  if (!args || typeof args !== "object") return [];
  const ids = (args as Record<string, unknown>).message_ids;
  return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === "string") : [];
}
