/*
 * Settings sub-page navigation. Each action lands on a settings section and
 * shows up in the palette under the "Settings" group.
 */

import { Palette as PaletteIcon } from "lucide-react";

import { useModals } from "@/state/modalStore";

import { getRuntimeNavigate } from "./runtime";
import type { ActionBehavior } from "./types";

interface SettingsRoute {
  id: string;
  to: string;
}

const ROUTES: SettingsRoute[] = [
  { id: "settings.theme", to: "/settings/theme" },
  { id: "settings.density", to: "/settings/density" },
  { id: "settings.keybindings", to: "/settings/keybindings" },
  { id: "settings.notifications", to: "/settings/notifications" },
  { id: "settings.compose", to: "/settings/compose" },
  { id: "settings.voice", to: "/settings/voice" },
  { id: "settings.llm", to: "/settings/llm" },
  { id: "settings.snippets", to: "/settings/snippets" },
  { id: "settings.token", to: "/settings/token" },
];

export const settingsActions: ActionBehavior[] = ROUTES.map(({ id, to }) => ({
  id,
  icon: PaletteIcon,
  run: () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  },
}));
