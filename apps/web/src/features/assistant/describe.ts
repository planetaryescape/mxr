/*
 * Reading what the assistant sends: the messages and draft a preview names.
 * A change's title and warning come from the daemon (docs/blueprint/29).
 */

type Json = Record<string, unknown>;

function asObject(value: unknown): Json {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Json) : {};
}

/** A tool name as words: `mxr_list_messages` reads "List messages". */
export function toolWords(tool: string): string {
  const words = tool.replace(/^mxr_/, "").replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
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
