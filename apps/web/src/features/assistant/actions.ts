/*
 * The assistant sidebar's keys and palette entries. Mod+. opens and closes
 * it from anywhere outside a text field.
 */

import { BotMessageSquare, SquarePen } from "lucide-react";

import { useAssistantUi } from "./assistantStore";
import type { Action } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

export const assistantActions: Action[] = [
  {
    id: "assistant.toggle",
    label: "Show or hide the assistant",
    shortLabel: "Assistant",
    description: "Talk to your mail: ask questions, or have it sort and tidy with your approval",
    group: "View",
    icon: BotMessageSquare,
    shortcut: "Mod+.",
    tuiNote: "Ctrl+a in the TUI, where terminals don't pass Ctrl+. through",
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      useAssistantUi.getState().toggle();
    },
  },
  {
    id: "assistant.new",
    label: "New assistant conversation",
    group: "View",
    icon: SquarePen,
    paletteOnly: true,
    run: () => {
      useModals.getState().setCommandPaletteOpen(false);
      const ui = useAssistantUi.getState();
      ui.newConversation();
      ui.setOpen(true);
    },
  },
];
