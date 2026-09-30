/*
 * Pure helpers for Reading and Paper trail: how bundles become a feed,
 * what the kind menu offers, and the words for sweeps and corrections.
 */

import type { MailKind, Place, PlaceBundle, PlaceMessage, SenderKind, SweepPreview } from "./api";
import { plural } from "@/lib/format";

export const PLACE_TITLES: Record<Place, string> = {
  reading: "Reading",
  paper_trail: "Paper trail",
};

/** Where each kind of sender's mail lives, in the words the menu uses. */
export const KIND_LABELS: Record<SenderKind, string> = {
  people: "People",
  reading: "Reading",
  paper_trail: "Paper trail",
  screened_out: "Screened out",
};

export interface KindOption {
  /** `null`: hand the sender back to the automatic rules. */
  kind: SenderKind | null;
  label: string;
  /** The key that picks this option while the menu is open. */
  key: string;
  hint: string;
}

export const KIND_OPTIONS: readonly KindOption[] = [
  { kind: "people", label: "People", key: "p", hint: "On the desk, like anyone you write to" },
  { kind: "reading", label: "Reading", key: "r", hint: "Newsletters and lists, read as a feed" },
  {
    kind: "paper_trail",
    label: "Paper trail",
    key: "t",
    hint: "Receipts and notifications, bundled",
  },
  { kind: "screened_out", label: "Screened out", key: "x", hint: "Off the desk and every place" },
  { kind: null, label: "Automatic", key: "a", hint: "Let the rules decide again" },
];

/** The option a key picks in the kind menu, if any. */
export function kindOptionForKey(key: string): KindOption | undefined {
  return KIND_OPTIONS.find((option) => option.key === key);
}

/** Who a bundle is from: the display name when there is one. */
export function bundleSender(bundle: Pick<PlaceBundle, "sender_name" | "sender_email">): string {
  return bundle.sender_name?.trim() || bundle.sender_email;
}

export function bundleKey(bundle: Pick<PlaceBundle, "account_id" | "sender_email">): string {
  return `${bundle.account_id}|${bundle.sender_email}`;
}

/** One issue in the Reading feed, with the bundle that says why it is there. */
export interface ReadingIssue {
  message: PlaceMessage;
  bundle: PlaceBundle;
}

/** Every listed issue across bundles, newest first. */
export function readingIssues(bundles: readonly PlaceBundle[]): ReadingIssue[] {
  return bundles
    .flatMap((bundle) => bundle.messages.map((message) => ({ message, bundle })))
    .toSorted(
      (a, b) =>
        Date.parse(b.message.date) - Date.parse(a.message.date) ||
        a.message.message_id.localeCompare(b.message.message_id),
    );
}

/** "Here because: automated sender, has List-Unsubscribe." */
export function whyHere(kind: Pick<MailKind, "reason">): string {
  return `Here because: ${kind.reason}.`;
}

/** A place or one sender's bundle in it, as the sweep dialog names it. */
export function sweepTitle(preview: SweepPreview, senderLabel?: string): string {
  const where = PLACE_TITLES[preview.place];
  if (preview.count === 0) {
    return senderLabel ? `Nothing to sweep from ${senderLabel}` : `Nothing to sweep in ${where}`;
  }
  const what = plural(preview.count, "message");
  return senderLabel ? `Archive ${what} from ${senderLabel}?` : `Archive ${what} from ${where}?`;
}

/**
 * The confirm button. A whole-place sweep names its scope and size, so it
 * never reads like one sender's bundle: "Archive all 143 from 35 senders".
 */
export function sweepConfirmLabel(preview: SweepPreview, wholePlace: boolean): string {
  if (!wholePlace) return `Archive ${plural(preview.count, "message")}`;
  return `Archive all ${preview.count.toLocaleString()} from ${plural(preview.senders.length, "sender")}`;
}

/**
 * The line under the title: how far the sweep reaches beyond the screen,
 * what stays, and how to get it back.
 */
export function sweepNote(preview: SweepPreview, shownHere?: number): string {
  const reach =
    shownHere !== undefined && shownHere < preview.count
      ? `Archives ${plural(preview.count, "message")}, ${shownHere.toLocaleString()} shown here. `
      : "";
  const stays =
    preview.pinned_excluded > 0
      ? `${plural(preview.pinned_excluded, "pinned message")} ${preview.pinned_excluded === 1 ? "stays" : "stay"}. `
      : "";
  return `${reach}${stays}Only what this preview found is archived; mail that arrives meanwhile stays. You can undo for about a minute.`;
}

/** The daemon refused a sweep because its preview expired or was used. */
export function isStalePreview(error: unknown): boolean {
  return error instanceof Error && /preview again/i.test(error.message);
}

/** "Moved Hacker News to People", "Hacker News is automatic again". */
export function correctionMessage(sender: string, kind: SenderKind | null): string {
  return kind ? `Moved ${sender} to ${KIND_LABELS[kind]}` : `${sender} is automatic again`;
}

/** One line of Paper trail: a sender's bundle, or a message of an open one. */
export type PlaceItem =
  | { type: "bundle"; key: string; bundle: PlaceBundle }
  | { type: "message"; key: string; bundle: PlaceBundle; message: PlaceMessage };

/** Bundles in order, each open bundle followed by its messages. */
export function paperTrailItems(
  bundles: readonly PlaceBundle[],
  expanded: ReadonlySet<string>,
): PlaceItem[] {
  return bundles.flatMap((bundle): PlaceItem[] => {
    const key = bundleKey(bundle);
    const head: PlaceItem = { type: "bundle", key, bundle };
    if (!expanded.has(key)) return [head];
    return [
      head,
      ...bundle.messages.map(
        (message): PlaceItem => ({
          type: "message",
          key: `${key}|${message.message_id}`,
          bundle,
          message,
        }),
      ),
    ];
  });
}
