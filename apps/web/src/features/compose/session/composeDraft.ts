/*
 * Pure draft-model helpers for a compose session: the in-memory draft shape,
 * its save fingerprint, local validation, and address parsing. No React, no
 * network, so every piece is directly testable.
 */

import type {
  ComposeFrontmatter,
  ComposeIssue,
  ComposeKind,
  ComposeSession,
  DraftAddress,
} from "../api";

export interface ComposeDraftState {
  draftPath: string;
  rawContent: string;
  frontmatter: ComposeFrontmatter;
  bodyMarkdown: string;
  issues: ComposeIssue[];
  accountId: string;
  kind: string;
  editorCommand?: string;
  cursorLine?: number;
}

export interface ComposeIntent {
  key: string;
  title: string;
  kind: ComposeKind;
  messageId?: string;
  draftId?: string;
  prefillTo?: string;
  prefillSubject?: string;
}

export interface ComposeSaveSnapshot {
  draftPath: string;
  accountId: string;
  fingerprint: string;
  frontmatter: ComposeFrontmatter;
  body: string;
}

export interface Snippet {
  name: string;
  body: string;
}

export function applyPrefill(
  draft: ComposeDraftState,
  intent: ComposeIntent,
): { draft: ComposeDraftState; changed: boolean } {
  if (intent.kind !== "new") return { draft, changed: false };
  const to = intent.prefillTo?.trim();
  const subject = intent.prefillSubject?.trim();
  let changed = false;
  const frontmatter = { ...draft.frontmatter };
  if (to && !frontmatter.to.trim()) {
    frontmatter.to = to;
    changed = true;
  }
  if (subject && !frontmatter.subject.trim()) {
    frontmatter.subject = subject;
    changed = true;
  }
  return changed ? { draft: { ...draft, frontmatter }, changed } : { draft, changed };
}

export function draftFromSession(
  session: ComposeSession,
  fallbackAccountId = "",
): ComposeDraftState {
  return {
    draftPath: session.draftPath,
    rawContent: session.rawContent,
    frontmatter: {
      to: session.frontmatter.to ?? "",
      cc: session.frontmatter.cc ?? "",
      bcc: session.frontmatter.bcc ?? "",
      subject: session.frontmatter.subject ?? "",
      from: session.frontmatter.from ?? "",
      attach: session.frontmatter.attach ?? [],
    },
    bodyMarkdown: session.bodyMarkdown ?? "",
    issues: session.issues ?? [],
    accountId: session.accountId ?? fallbackAccountId,
    kind: session.kind ?? "new",
    editorCommand: session.editorCommand,
    cursorLine: session.cursorLine,
  };
}

export function captureSaveSnapshot(draft: ComposeDraftState): ComposeSaveSnapshot {
  return {
    draftPath: draft.draftPath,
    accountId: draft.accountId,
    fingerprint: draftFingerprint(draft),
    frontmatter: { ...draft.frontmatter, attach: [...draft.frontmatter.attach] },
    body: draft.bodyMarkdown,
  };
}

export function composeQueueKey(draftPath: string): string {
  return `compose:${draftPath}`;
}

export function draftFingerprint(draft: ComposeDraftState): string {
  return JSON.stringify({
    to: draft.frontmatter.to,
    cc: draft.frontmatter.cc,
    bcc: draft.frontmatter.bcc,
    subject: draft.frontmatter.subject,
    from: draft.frontmatter.from,
    attach: draft.frontmatter.attach,
    body: draft.bodyMarkdown,
  });
}

export function localComposeIssues(draft: ComposeDraftState): ComposeIssue[] {
  const issues: ComposeIssue[] = [];
  if (!draft.frontmatter.to.trim())
    issues.push({ severity: "error", message: "No recipients (to: field is empty)" });
  for (const address of splitAddresses(
    `${draft.frontmatter.to},${draft.frontmatter.cc},${draft.frontmatter.bcc}`,
  )) {
    if (!address.includes("@"))
      issues.push({ severity: "error", message: `Invalid email address: ${address}` });
  }
  if (!draft.frontmatter.subject.trim())
    issues.push({ severity: "warning", message: "Subject is empty" });
  if (!draft.bodyMarkdown.trim())
    issues.push({ severity: "warning", message: "Message body is empty" });
  return issues;
}

export function splitAddresses(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

export function firstAddress(value: string): string | undefined {
  const first = splitAddresses(value)[0];
  if (!first) return undefined;
  const match = first.match(/<([^>]+)>/);
  return (match?.[1] ?? first).trim() || undefined;
}

export function draftIntentFromKind(kind: string): ComposeKind {
  return kind === "reply" || kind === "reply_all" || kind === "forward" ? kind : "new";
}

/** "Name <email>" / bare-email chips → daemon `Address` values. */
export function parseDraftAddresses(value: string): DraftAddress[] {
  return splitAddresses(value).map((raw) => {
    const match = raw.match(/^(.*?)\s*<([^>]+)>$/);
    if (match?.[2]) return { name: match[1]?.trim() || null, email: match[2].trim() };
    return { name: null, email: raw };
  });
}

export function countRecipients(frontmatter: ComposeFrontmatter): number {
  return splitAddresses(`${frontmatter.to},${frontmatter.cc},${frontmatter.bcc}`).length;
}

export function expandSnippet(value: string, snippets: Snippet[]): string {
  const match = value.match(/(^|\s);([A-Za-z0-9_-]+) $/);
  if (!match) return value;
  const snippet = snippets.find((item) => item.name === match[2]);
  if (!snippet) return value;
  return `${value.slice(0, match.index)}${match[1] ?? ""}${snippet.body}`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
