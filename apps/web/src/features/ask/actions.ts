/*
 * "Ask your archive": a palette action that opens the right-rail question
 * box. Palette-only, like the TUI's command-palette entry.
 */

import { MessageCircleQuestion } from "lucide-react";

import type { Action } from "@/lib/actions/types";
import { useModals } from "@/state/modalStore";

export const askActions: Action[] = [
  {
    id: "ask.archive",
    label: "Ask your archive",
    description: "Ask a question and get an answer with the messages it came from",
    group: "Search",
    icon: MessageCircleQuestion,
    paletteOnly: true,
    run: () => {
      const modals = useModals.getState();
      modals.setCommandPaletteOpen(false);
      modals.openRightRail("ask-archive");
    },
  },
];
