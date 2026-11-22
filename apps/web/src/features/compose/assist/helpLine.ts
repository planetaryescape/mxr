import type { ComposeAssistAction } from "./types";

export interface HelpLineState {
  hasText: boolean;
  hasSelection: boolean;
  answersMessage: boolean;
}

export function helpPlaceholder(state: HelpLineState): string {
  if (state.hasSelection) return "Edit the selection: say how";
  if (state.hasText) return "Finish my draft, or say what to change";
  return state.answersMessage
    ? "Draft a reply, or say what it should say"
    : "Draft this email, or say what it should say";
}

export function helpAction(state: HelpLineState): Exclude<ComposeAssistAction, "polish"> {
  if (state.hasSelection) return "edit_selection";
  return state.hasText ? "finish" : "write";
}
