/*
 * The assistant sidebar's keys and palette entries. Its key opens and
 * closes it from anywhere outside a text field.
 */

import { CircleDot, SquarePen } from "lucide-react";

import { useAssistantUi } from "./assistantStore";
import { definedKeys, definedShortcut } from "@/lib/actions/definition";
import type { ActionBehavior } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

/**
 * The toggle's key for the sidebar's own hints: the modifier chord, since
 * the sidebar's message box types `@`.
 */
export const ASSISTANT_SHORTCUT =
  definedKeys("assistant.toggle").find((key) => key.startsWith("Mod+")) ??
  definedShortcut("assistant.toggle");

export const assistantActions: ActionBehavior[] = [
  {
    id: "assistant.toggle",
    icon: CircleDot,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      useAssistantUi.getState().toggle("key");
    },
  },
  {
    id: "assistant.new",
    icon: SquarePen,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      const ui = useAssistantUi.getState();
      ui.newConversation();
      ui.setOpen(true, "key");
    },
  },
];
