import type { KeyboardEvent } from "react";

import { definedKeys } from "@/lib/actions/definition";
import type { ActionBehavior } from "@/lib/actions/types";
import { parseChord } from "@/lib/keys/chord";

export interface ComposeShortcutHandlers {
  busy: boolean;
  openDraftHelp: () => void;
  canServerSave: boolean;
  revealCc: () => void;
  revealBcc: () => void;
  handleAttachShortcut: () => void;
  requestSendLater: () => void;
  openSignaturePicker: () => void;
  handleRefreshClick: () => Promise<void>;
  handleServerSaveClick: () => Promise<void>;
  openSnippetPicker: () => void;
  handleSaveClick: () => Promise<void>;
  requestSendAndArchive: () => void;
  requestSend: () => void;
  requestDiscard: () => void;
}

type Command = Exclude<keyof ComposeShortcutHandlers, "busy" | "canServerSave">;

const COMPOSE_COMMANDS = {
  "compose.send": "requestSend",
  "compose.send-and-archive": "requestSendAndArchive",
  "compose.send-later": "requestSendLater",
  "compose.save": "handleSaveClick",
  "compose.save-to-server": "handleServerSaveClick",
  "compose.refresh": "handleRefreshClick",
  "compose.discard": "requestDiscard",
  "compose.add-cc": "revealCc",
  "compose.add-bcc": "revealBcc",
  "compose.attach": "handleAttachShortcut",
  "compose.signature": "openSignaturePicker",
  "compose.snippets": "openSnippetPicker",
  "compose.draft-for-me": "openDraftHelp",
} as const satisfies Record<string, Command>;

export const composeKeyActions: ActionBehavior[] = [
  ...Object.entries(COMPOSE_COMMANDS).map(([id, command]) => ({ id, command })),
  { id: "compose.fullscreen", command: "toggleFullscreen" },
];

function eventChord(chord: string): string {
  const match = /^Mod\+(Shift\+)?(.+)$/.exec(chord);
  if (!match?.[2])
    throw new Error(`Compose chord "${chord}" must be Mod, optional Shift, and a key`);
  return `${match[1] ? "shift+" : ""}${match[2].toLowerCase()}`;
}

export const COMPOSE_CHORDS: Readonly<Record<string, Command>> = Object.fromEntries(
  Object.entries(COMPOSE_COMMANDS).flatMap(([id, command]) =>
    definedKeys(id).map((chord) => [eventChord(chord), command] as const),
  ),
);

const WAITS_WHILE_BUSY = new Set<Command>([
  "requestSendLater",
  "handleRefreshClick",
  "handleServerSaveClick",
  "handleSaveClick",
  "requestSendAndArchive",
  "requestSend",
  "requestDiscard",
]);

interface ChordEvent {
  target?: EventTarget | null;
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
}

export function composeCommandOf(event: ChordEvent): Command | null {
  const chord = chordOf(event);
  const command =
    chord !== null && Object.hasOwn(COMPOSE_CHORDS, chord) ? (COMPOSE_CHORDS[chord] ?? null) : null;
  if (
    command === "requestDiscard" &&
    typeof Element !== "undefined" &&
    event.target instanceof Element &&
    event.target.closest("input, textarea, [contenteditable='true']")
  )
    return null;
  return command;
}

export function isComposeChord(event: ChordEvent): boolean {
  return composeCommandOf(event) !== null;
}

export function isComposeActionChord(event: ChordEvent, id: string): boolean {
  const chord = chordOf(event);
  return chord !== null && definedKeys(id).map(eventChord).includes(chord);
}

function chordOf(event: ChordEvent): string | null {
  if (!(event.metaKey || event.ctrlKey)) return null;
  return `${event.shiftKey ? "shift+" : ""}${event.key.toLowerCase()}`;
}

export function codeMirrorKey(chord: string): string {
  return parseChord(chord)
    .map((token) => {
      const parts = token.split("+");
      if (parts.at(-1) === "") return [...parts.slice(0, -2), "+"].join("-");
      return parts.join("-");
    })
    .join(" ");
}

export function handleComposeShortcut(
  event: KeyboardEvent<HTMLDivElement>,
  handlers: ComposeShortcutHandlers,
) {
  if (event.defaultPrevented) return;
  const command = composeCommandOf(event);
  if (!command) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.repeat) return;
  if (WAITS_WHILE_BUSY.has(command) && handlers.busy) return;
  if (command === "handleServerSaveClick" && !handlers.canServerSave) return;
  void handlers[command]();
}
