/*
 * Words for what the assistant does: a title for each change it asks to
 * make, and the messages a preview names, so an approval reads as "Archive
 * 3 messages" with the three listed rather than a tool call.
 */

type Json = Record<string, unknown>;

/** Each verb around its object, e.g. "Move 3 messages to Trash". */
const MUTATION_PHRASES: Record<string, (things: string) => string> = {
  archive: (things) => `Archive ${things}`,
  read_and_archive: (things) => `Mark ${things} read and archive`,
  trash: (things) => `Move ${things} to Trash`,
  spam: (things) => `Report ${things} as spam`,
  mark_read: (things) => `Mark ${things} read`,
  mark_unread: (things) => `Mark ${things} unread`,
  star: (things) => `Star ${things}`,
  unstar: (things) => `Unstar ${things}`,
};

function count(value: unknown, one: string): string {
  const n = Array.isArray(value) ? value.length : 0;
  if (n === 0) return `${one}s`;
  return `${n} ${n === 1 ? one : `${one}s`}`;
}

function asObject(value: unknown): Json {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Json) : {};
}

/** A tool name as words: `mxr_list_messages` reads "List messages". */
export function toolWords(tool: string): string {
  const words = tool.replace(/^mxr_/, "").replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The title of an approval card. */
export function describeChange(tool: string, args: unknown): string {
  const input = asObject(args);
  switch (tool) {
    case "mxr_mutate": {
      const things = count(input.message_ids, "message");
      const phrase = MUTATION_PHRASES[String(input.action)];
      return phrase ? phrase(things) : `${toolWords(String(input.action ?? tool))} ${things}`;
    }
    case "mxr_send_draft":
      return "Send a draft";
    case "mxr_archive_submit":
      return "Archive the reviewed messages";
    case "mxr_move":
      return input.sender
        ? `Send this sender's mail to ${String(input.mode ?? "another mode")}`
        : `Move an email to ${String(input.mode ?? "another mode")}`;
    case "mxr_todo_set_state":
      return `${toolWords(String(input.action ?? "change"))} ${count(input.todo_ids, "to-do")}`;
    case "mxr_delete_draft":
      return "Delete a draft";
    case "mxr_update_draft":
      return "Replace a draft's content";
    case "mxr_got_it":
      return "Send a short acknowledgement";
    case "mxr_sorting_rule":
      return "Save a sorting rule";
    case "mxr_category_rule":
      return input.delete ? "Delete a category rule" : "Save a category rule";
    case "mxr_category_create":
      return `Create the category "${String(input.name ?? "")}"`;
    case "mxr_category_rename":
      return `Rename a category to "${String(input.name ?? "")}"`;
    case "mxr_category_delete":
      return "Delete a category";
    case "mxr_category_assignment_apply":
      return "Change message categories";
    case "mxr_reading_later":
      return input.later === false ? "Take items off Later" : "Put items on Later";
    default:
      return toolWords(tool);
  }
}

export interface PreviewMessage {
  key: string;
  from: string;
  subject: string;
  date?: string;
}

/**
 * The messages a preview names, wherever the tool put them: the first list
 * of objects with a subject, a few levels down.
 */
export function previewMessages(preview: unknown): PreviewMessage[] {
  const list = findSubjectList(preview, 0);
  if (!list) return [];
  return list.map((item, index) => {
    const row = asObject(item);
    const from = asObject(row.from);
    return {
      key: String(row.id ?? row.message_id ?? index),
      from: String(from.name || from.email || row.from || row.sender || ""),
      subject: String(row.subject || "(no subject)"),
      date: typeof row.date === "string" ? row.date : undefined,
    };
  });
}

function findSubjectList(value: unknown, depth: number): unknown[] | null {
  if (depth > 4 || value === null || typeof value !== "object") return null;
  if (Array.isArray(value)) {
    if (value.length > 0 && value.every((item) => "subject" in asObject(item))) return value;
    for (const item of value) {
      const found = findSubjectList(item, depth + 1);
      if (found) return found;
    }
    return null;
  }
  for (const child of Object.values(value)) {
    const found = findSubjectList(child, depth + 1);
    if (found) return found;
  }
  return null;
}

/** A draft a preview shows (sending one previews it whole). */
export function previewDraft(
  preview: unknown,
): { to: string; subject: string; body: string } | null {
  const draft = asObject(asObject(preview).draft ?? preview);
  if (!("subject" in draft) && !("body" in draft)) return null;
  const to = Array.isArray(draft.to)
    ? draft.to
        .map((address) => {
          const entry = asObject(address);
          return String(entry.email ?? address);
        })
        .join(", ")
    : String(draft.to ?? "");
  return {
    to,
    subject: String(draft.subject ?? ""),
    body: String(draft.body ?? draft.body_markdown ?? ""),
  };
}
