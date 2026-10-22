/*
 * "Ask your archive": a palette action that opens the right-rail question
 * box. Palette-only, like the TUI's command-palette entry.
 */

import { MessageCircleQuestion } from "lucide-react";

import type { ActionBehavior } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

export const askActions: ActionBehavior[] = [
  {
    id: "ask.archive",
    icon: MessageCircleQuestion,
    run: () => {
      const modals = useModals.getState();
      modals.setCommandPaletteOpen(false);
      modals.openRightRail("ask-archive");
    },
  },
];
