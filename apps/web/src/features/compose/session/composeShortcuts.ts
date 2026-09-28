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

export function handleComposeShortcut(
  event: KeyboardEvent<HTMLDivElement>,
  handlers: ComposeShortcutHandlers,
) {
  const { busy, canServerSave } = handlers;
  if (event.defaultPrevented) return;
  if (!(event.metaKey || event.ctrlKey)) return;
  const key = event.key.toLowerCase();
  if (event.repeat) {
    // A held chord runs once: a held ⌘↵ must not queue sends behind the lock,
    // nor a held ⌘⌫ reopen the discard prompt.
    if (isComposeChord(event)) {
      event.preventDefault();
      event.stopPropagation();
    }
    return;
  }

  if (event.shiftKey && key === "c") {
    event.preventDefault();
    event.stopPropagation();
    handlers.revealCc();
    return;
  }
  if (event.shiftKey && key === "b") {
    event.preventDefault();
    event.stopPropagation();
    handlers.revealBcc();
    return;
  }
  if (event.shiftKey && key === "a") {
    event.preventDefault();
    event.stopPropagation();
    handlers.handleAttachShortcut();
    return;
  }
  if (event.shiftKey && key === "l") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) handlers.requestSendLater();
    return;
  }
  if (event.shiftKey && key === "g") {
    event.preventDefault();
    event.stopPropagation();
    handlers.openSignaturePicker();
    return;
  }
  if (event.shiftKey && key === "r") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) void handlers.handleRefreshClick();
    return;
  }
  if (event.shiftKey && key === "s") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy && canServerSave) void handlers.handleServerSaveClick();
    return;
  }
  if (!event.shiftKey && key === ";") {
    event.preventDefault();
    event.stopPropagation();
    handlers.openSnippetPicker();
    return;
  }
  if (!event.shiftKey && key === "s") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) void handlers.handleSaveClick();
    return;
  }
  if (event.shiftKey && event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) handlers.requestSendAndArchive();
    return;
  }
  if (!event.shiftKey && event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) handlers.requestSend();
    return;
  }
  if (!event.shiftKey && event.key === "Backspace") {
    event.preventDefault();
    event.stopPropagation();
    if (!busy) handlers.requestDiscard();
  }
}

/** Keys the compose surface binds under ⌘/Ctrl (with or without Shift). */
const COMPOSE_KEYS = new Set(["c", "b", "a", "l", "g", "r", "s", ";", "enter", "backspace"]);

/**
 * A ⌘/Ctrl chord the composer owns. Every compose surface (this handler and
 * both editors) swallows its repeats, so a held chord runs once.
 */
export function isComposeChord(event: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
}): boolean {
  return (event.metaKey || event.ctrlKey) && COMPOSE_KEYS.has(event.key.toLowerCase());
}
