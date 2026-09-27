/*
 * Mail lenses: the shell's sidebar items turned into one typed list with a
 * URL, fetch parameters and an identity for optimistic projection. The
 * sidebar, the mailbox query, `g 1`-`g 9` and the palette all resolve
 * lenses here, so a label's URL slug and its real name never get mixed up.
 */

import type { LensIdentity } from "@/features/mail-actions/pendingMailOps";

import type { MailboxLensParams } from "./api";
import type { ShellResponse, SidebarItem } from "./types";

export type LensRoute =
  | { kind: "system"; mailbox: string }
  | { kind: "label"; name: string }
  | { kind: "saved"; slug: string };

export interface MailLens {
  /** Stable key: "inbox", "label:work", "saved:follow-ups". */
  key: string;
  label: string;
  path: string;
  section: "system" | "labels" | "saved";
  unread: number;
  total: number;
  params: MailboxLensParams;
  identity: LensIdentity;
  /** Real label name for label lenses (moves, routes, projections). */
  labelName?: string;
}

const SYSTEM_ORDER = ["inbox", "starred", "sent", "drafts", "archive", "spam", "trash"];

export function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function systemSlug(item: SidebarItem): string {
  if (item.lens?.kind === "inbox") return "inbox";
  if (item.lens?.kind === "all_mail") return "archive";
  return slugify(item.label);
}

function identityForSystem(slug: string, labelName: string): LensIdentity {
  switch (slug) {
    case "inbox":
      return { kind: "inbox" };
    case "archive":
      return { kind: "all_mail" };
    case "starred":
      return { kind: "starred" };
    case "trash":
      return { kind: "trash" };
    case "spam":
      return { kind: "spam" };
    default:
      return { kind: "label", labelName };
  }
}

export function lensesFromShell(shell: ShellResponse | undefined): MailLens[] {
  const lenses: MailLens[] = [];
  for (const section of shell?.sidebar?.sections ?? []) {
    for (const item of section.items) {
      const lens = lensFromItem(section.id, item);
      if (lens) lenses.push(lens);
    }
  }
  const systemRank = (lens: MailLens) => {
    const index = SYSTEM_ORDER.indexOf(lens.key);
    return index < 0 ? SYSTEM_ORDER.length : index;
  };
  return lenses.toSorted((a, b) =>
    a.section === "system" && b.section === "system" ? systemRank(a) - systemRank(b) : 0,
  );
}

function lensFromItem(sectionId: string, item: SidebarItem): MailLens | null {
  const lens = item.lens;
  const counts = { unread: item.unread ?? 0, total: item.total ?? 0 };
  if (!lens) return null;
  if (sectionId === "system" || lens.kind === "inbox" || lens.kind === "all_mail") {
    // Subscriptions has its own page with unsubscribe actions.
    if (lens.kind === "subscription") return null;
    const slug = systemSlug(item);
    const params: MailboxLensParams =
      lens.kind === "inbox"
        ? { lens_kind: "inbox" }
        : lens.kind === "all_mail"
          ? { lens_kind: "all_mail" }
          : { lens_kind: "label", label_id: lens.labelId ?? undefined };
    return {
      key: slug,
      label: displaySystemName(item.label, slug),
      path: `/m/${slug}`,
      section: "system",
      ...counts,
      params,
      identity: identityForSystem(slug, item.label),
      labelName: lens.kind === "label" ? item.label : undefined,
    };
  }
  if (lens.kind === "label" && lens.labelId) {
    return {
      key: `label:${item.id}`,
      label: item.label,
      path: `/m/label/${encodeURIComponent(item.id)}`,
      section: "labels",
      ...counts,
      params: { lens_kind: "label", label_id: lens.labelId },
      identity: { kind: "label", labelName: item.label },
      labelName: item.label,
    };
  }
  if (lens.kind === "saved_search" && lens.savedSearch) {
    const slug = item.id.replace(/^saved-search-/, "");
    return {
      key: `saved:${slug}`,
      label: item.label,
      path: `/m/saved/${encodeURIComponent(slug)}`,
      section: "saved",
      ...counts,
      params: { lens_kind: "saved_search", saved_search: lens.savedSearch },
      identity: { kind: "other" },
    };
  }
  return null;
}

/** Gmail system labels arrive upper-case ("SENT"); show them as words. */
function displaySystemName(name: string, slug: string): string {
  if (slug === "archive") return "All Mail";
  if (name !== name.toUpperCase()) return name;
  return name.charAt(0) + name.slice(1).toLowerCase();
}

/**
 * The lens a route names, or null when the shell has no such lens (a
 * deleted label, a typo'd URL). Callers show that honestly rather than
 * falling back to some other mailbox.
 */
export function resolveLens(route: LensRoute, lenses: MailLens[]): MailLens | null {
  if (route.kind === "system") {
    const slug = route.mailbox === "all-mail" ? "archive" : route.mailbox;
    return (
      lenses.find((lens) => lens.section === "system" && lens.key === slug) ??
      (slug === "inbox" ? INBOX_FALLBACK : null)
    );
  }
  if (route.kind === "label") {
    const target = decodeURIComponent(route.name);
    return (
      lenses.find(
        (lens) =>
          lens.section === "labels" &&
          (lens.key === `label:${target}` || slugify(lens.label) === slugify(target)),
      ) ?? null
    );
  }
  const slug = decodeURIComponent(route.slug);
  return (
    lenses.find(
      (lens) =>
        lens.section === "saved" && (lens.key === `saved:${slug}` || slugify(lens.label) === slug),
    ) ?? null
  );
}

/** Inbox works before the shell has loaded (first paint, empty accounts). */
export const INBOX_FALLBACK: MailLens = {
  key: "inbox",
  label: "Inbox",
  path: "/m/inbox",
  section: "system",
  unread: 0,
  total: 0,
  params: { lens_kind: "inbox" },
  identity: { kind: "inbox" },
};

export function savedSearchLenses(lenses: MailLens[]): MailLens[] {
  return lenses.filter((lens) => lens.section === "saved");
}
