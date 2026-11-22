import type { MailKind, Place, PlaceBundle, PlaceMessage, SenderKind, SweepPreview } from "./api";
import { plural } from "@/lib/format";
import { optionLetter } from "@/lib/keys/overlayKeys";

export const PLACE_TITLES: Record<Place, string> = {
  reading: "Reading",
  paper_trail: "Paper trail",
};

export const KIND_LABELS: Record<SenderKind, string> = {
  people: "People",
  reading: "Reading",
  paper_trail: "Paper trail",
  screened_out: "Screened out",
};

export interface KindOption {
  kind: SenderKind | null;
  label: string;
  key: string;
  hint: string;
}

export const KIND_OPTIONS: readonly KindOption[] = [
  {
    kind: "people",
    label: "People",
    key: optionLetter("sender-kind-picker.people"),
    hint: "On the desk, like anyone you write to",
  },
  {
    kind: "reading",
    label: "Reading",
    key: optionLetter("sender-kind-picker.reading"),
    hint: "Newsletters and lists, read as a feed",
  },
  {
    kind: "paper_trail",
    label: "Paper trail",
    key: optionLetter("sender-kind-picker.paper-trail"),
    hint: "Receipts and notifications, bundled",
  },
  {
    kind: "screened_out",
    label: "Screened out",
    key: optionLetter("sender-kind-picker.screened-out"),
    hint: "Off the desk and every place",
  },
  {
    kind: null,
    label: "Automatic",
    key: optionLetter("sender-kind-picker.automatic"),
    hint: "Let the rules decide again",
  },
];

export function kindOptionForKey(key: string): KindOption | undefined {
  return KIND_OPTIONS.find((option) => option.key === key.toLowerCase());
}

export function bundleSender(bundle: Pick<PlaceBundle, "sender_name" | "sender_email">): string {
  return bundle.sender_name?.trim() || bundle.sender_email;
}

export function bundleKey(bundle: Pick<PlaceBundle, "account_id" | "sender_email">): string {
  return `${bundle.account_id}|${bundle.sender_email}`;
}

export function whyHere(kind: Pick<MailKind, "reason">): string {
  return `Here because: ${kind.reason}.`;
}

export function sweepTitle(preview: SweepPreview, senderLabel?: string): string {
  const where = PLACE_TITLES[preview.place];
  if (preview.count === 0) {
    return senderLabel ? `Nothing to sweep from ${senderLabel}` : `Nothing to sweep in ${where}`;
  }
  const what = plural(preview.count, "message");
  return senderLabel ? `Archive ${what} from ${senderLabel}?` : `Archive ${what} from ${where}?`;
}

export function sweepConfirmLabel(preview: SweepPreview, wholePlace: boolean): string {
  if (!wholePlace) return `Archive ${plural(preview.count, "message")}`;
  return `Archive all ${preview.count.toLocaleString()} from ${plural(preview.senders.length, "sender")}`;
}

export function sweepNote(preview: SweepPreview, shownHere?: number): string {
  const reach =
    shownHere !== undefined && shownHere > 0 && shownHere < preview.count
      ? `Archives ${plural(preview.count, "message")}, ${shownHere.toLocaleString()} shown here. `
      : "";
  const stays =
    preview.pinned_excluded > 0
      ? `${plural(preview.pinned_excluded, "pinned message")} ${preview.pinned_excluded === 1 ? "stays" : "stay"}. `
      : "";
  return `${reach}${stays}Only what this preview found is archived; mail that arrives meanwhile stays. You can undo for about a minute.`;
}

export function isStalePreview(error: unknown): boolean {
  return error instanceof Error && /preview again/i.test(error.message);
}

export function correctionMessage(sender: string, kind: SenderKind | null): string {
  return kind ? `Moved ${sender} to ${KIND_LABELS[kind]}` : `${sender} is automatic again`;
}

export type PlaceItem =
  | { type: "bundle"; key: string; bundle: PlaceBundle }
  | { type: "message"; key: string; bundle: PlaceBundle; message: PlaceMessage };

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
