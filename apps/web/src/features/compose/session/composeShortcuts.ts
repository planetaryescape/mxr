/*
 * Cmd/Ctrl shortcuts inside the compose surface. Kept as a plain function
 * over a handler bag so the key table reads in one place.
 */

import type { KeyboardEvent } from "react";

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
 * mxr's compose chords under ⌘/Ctrl ("shift+" when Shift is part of the
 * chord), and what each runs. The one source: the compose handler
 * dispatches from it and every editor's repeat guard reads it. Anything
 * not here (⌘B, ⌘Z, ⌘⇧Z, clipboard, selection) belongs to the editor.
 */
export const COMPOSE_CHORDS = {
  "shift+c": "revealCc",
  "shift+b": "revealBcc",
  "shift+a": "handleAttachShortcut",
  "shift+l": "requestSendLater",
  "shift+g": "openSignaturePicker",
  "shift+r": "handleRefreshClick",
  "shift+s": "handleServerSaveClick",
  s: "handleSaveClick",
  ";": "openSnippetPicker",
  "shift+enter": "requestSendAndArchive",
  enter: "requestSend",
  backspace: "requestDiscard",
  "shift+backspace": "requestDiscard",
} as const satisfies Record<string, Command>;

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
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
}

/** The compose command a key event is, or null for anything else. */
export function composeCommandOf(event: ChordEvent): Command | null {
  if (!(event.metaKey || event.ctrlKey)) return null;
  const chord = `${event.shiftKey ? "shift+" : ""}${event.key.toLowerCase()}`;
  return Object.hasOwn(COMPOSE_CHORDS, chord)
    ? COMPOSE_CHORDS[chord as keyof typeof COMPOSE_CHORDS]
    : null;
}

/**
 * One of mxr's compose chords. Every compose surface swallows only their
 * repeats, so a held ⌘↵ sends once while a held ⌘Z keeps undoing.
 */
export function isComposeChord(event: ChordEvent): boolean {
  return composeCommandOf(event) !== null;
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
