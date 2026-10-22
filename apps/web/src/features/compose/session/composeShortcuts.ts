/*
 * Cmd/Ctrl shortcuts inside the compose surface. Kept as a plain function
 * over a handler bag so the key table reads in one place.
 */

import type { KeyboardEvent } from "react";

import { definedKeys } from "@/lib/actions/definition";
import type { ActionBehavior } from "@/lib/actions/types";
import { parseChord } from "@/lib/keys/chord";

export interface ComposeShortcutHandlers {
  busy: boolean;
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

/**
 * mxr's compose actions, by their id in the key definition
 * (`crates/keymap/keymap.toml`), and what each runs. The definition holds
 * the keys; this holds the behaviour.
 */
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
} as const satisfies Record<string, Command>;

/**
 * The compose actions for the registry, so help, the docs and the
 * browser-reserved check see their keys. Compose owns the keyboard while
 * it is open and the dispatcher stands aside, so nothing pushes the
 * `compose` scope or registers its controller: `command` names the
 * handler below, and the palette never offers these.
 */
export const composeKeyActions: ActionBehavior[] = [
  ...Object.entries(COMPOSE_COMMANDS).map(([id, command]) => ({ id, command })),
  { id: "compose.fullscreen", command: "toggleFullscreen" },
];

/**
 * A definition chord in the form `chordOf` gives a key event: "Mod+Shift+c"
 * → "shift+c". Compose takes ⌘ or Ctrl as Mod on every platform, so only
 * Shift and the key are compared.
 */
function eventChord(chord: string): string {
  const match = /^Mod\+(Shift\+)?(.+)$/.exec(chord);
  if (!match?.[2])
    throw new Error(`Compose chord "${chord}" must be Mod, optional Shift, and a key`);
  return `${match[1] ? "shift+" : ""}${match[2].toLowerCase()}`;
}

/**
 * mxr's compose chords under ⌘/Ctrl ("shift+" when Shift is part of the
 * chord), and what each runs. The one source: the compose handler
 * dispatches from it and every editor's repeat guard reads it. Anything
 * not here (⌘B, ⌘Z, ⌘⇧Z, clipboard, selection) belongs to the editor.
 */
export const COMPOSE_CHORDS: Readonly<Record<string, Command>> = Object.fromEntries(
  Object.entries(COMPOSE_COMMANDS).flatMap(([id, command]) =>
    definedKeys(id).map((chord) => [eventChord(chord), command] as const),
  ),
);

/** Commands that wait while the draft is busy (saving, sending, loading). */
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

/** The compose command a key event is, or null for anything else. */
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

/**
 * One of mxr's compose chords. Every compose surface swallows only their
 * repeats, so a held ⌘↵ sends once while a held ⌘Z keeps undoing.
 */
export function isComposeChord(event: ChordEvent): boolean {
  return composeCommandOf(event) !== null;
}

/** True when `event` is one of the defined chords of compose action `id`. */
export function isComposeActionChord(event: ChordEvent, id: string): boolean {
  const chord = chordOf(event);
  return chord !== null && definedKeys(id).map(eventChord).includes(chord);
}

function chordOf(event: ChordEvent): string | null {
  if (!(event.metaKey || event.ctrlKey)) return null;
  return `${event.shiftKey ? "shift+" : ""}${event.key.toLowerCase()}`;
}

/**
 * A definition chord in CodeMirror's key notation: "Mod+Enter" →
 * "Mod-Enter". CodeMirror reads Mod as ⌘ on macOS and Ctrl elsewhere, as
 * the definition does.
 */
export function codeMirrorKey(chord: string): string {
  return parseChord(chord)
    .map((token) => {
      const parts = token.split("+");
      // "+" itself as a key arrives as a trailing empty part.
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
  // A held chord runs once: a held ⌘↵ must not queue sends behind the
  // lock, nor a held ⌘⌫ reopen the discard prompt.
  if (event.repeat) return;
  if (WAITS_WHILE_BUSY.has(command) && handlers.busy) return;
  if (command === "handleServerSaveClick" && !handlers.canServerSave) return;
  void handlers[command]();
}
