/*
 * What the help line says and what it sends, from the state of the draft.
 * Placeholder and action come from the same three facts (has text, has a
 * selection, is a reply), so the line never promises one thing and does
 * another.
 */

import type { ComposeAssistAction } from "./types";

export interface HelpLineState {
  /** The user's text region has something in it. */
  hasText: boolean;
  /** A non-empty selection sits inside the text region. */
  hasSelection: boolean;
  /** The draft answers a message (a reply or forward), not a new email. */
  answersMessage: boolean;
}

export function helpPlaceholder(state: HelpLineState): string {
  if (state.hasSelection) return "Edit the selection: say how";
  if (state.hasText) return "Finish my draft, or say what to change";
  return state.answersMessage
    ? "Draft a reply, or say what it should say"
    : "Draft this email, or say what it should say";
}

/** The action a plain submit sends, given the state of the draft. */
export function helpAction(state: HelpLineState): Exclude<ComposeAssistAction, "polish"> {
  if (state.hasSelection) return "edit_selection";
  return state.hasText ? "finish" : "write";
}
