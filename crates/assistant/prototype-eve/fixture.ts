// THROWAWAY: file-backed fake mailbox, never imports mxr or reads real mail.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";

export const root = resolve(process.env.SPIKE_STATE ?? ".prototype-state");
export const path = resolve(root, "mailbox.json");
export function initial() {
  const previews: Record<string, string[]> = {};
  const archiveExecutions: string[][] = [];
  const sends: Record<string, { to: string; body: string }> = {};
  return {
    messages: [
      { id: "shipping-1", subject: "Shipping notice: parcel one", archived: false },
      { id: "shipping-2", subject: "Shipping notice: parcel two", archived: false },
      { id: "personal-1", subject: "Lunch tomorrow?", archived: false },
    ],
    previews,
    archiveExecutions,
    sends,
    sendAttempts: 0,
  };
}
export function load(): ReturnType<typeof initial> {
  return JSON.parse(readFileSync(path, "utf8"));
}
export function save(state: ReturnType<typeof initial>) {
  mkdirSync(root, { recursive: true });
  writeFileSync(`${path}.new`, JSON.stringify(state, null, 2));
  renameSync(`${path}.new`, path);
}
export function search(query: string) {
  return {
    source: "untrusted-mail",
    messages: load().messages.filter((m) => !m.archived && m.subject.toLowerCase().includes(query.toLowerCase())),
  };
}
export function archive(ids: string[], dryRun: boolean, token?: string) {
  const state = load();
  const selected = [...new Set(ids)].sort();
  if (selected.some((id) => !state.messages.some((m) => m.id === id))) throw new Error("Unknown message");
  if (dryRun) {
    const previewToken = createHash("sha256").update(JSON.stringify(selected)).digest("hex");
    state.previews[previewToken] = selected;
    save(state);
    return { dry_run: true, ids: selected, preview_token: previewToken };
  }
  if (!token || JSON.stringify(state.previews[token]) !== JSON.stringify(selected)) {
    throw new Error("Preview token does not bind this exact selection");
  }
  // Consumed token makes an accidental retry fail closed.
  delete state.previews[token];
  for (const m of state.messages) if (selected.includes(m.id)) m.archived = true;
  state.archiveExecutions.push(selected);
  save(state);
  return { archived: selected };
}
