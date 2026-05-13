/*
 * Settings sub-page navigation. Each action lands on a settings section and
 * shows up in the palette under the "Settings" group.
 */

import { Palette as PaletteIcon } from "lucide-react";

import { useModals } from "@/state/modalStore";

import { getRuntimeNavigate } from "./runtime";
import type { Action } from "./types";

interface SettingsRoute {
  id: string;
  label: string;
  description: string;
  to: string;
}

const ROUTES: SettingsRoute[] = [
  {
    id: "settings.theme",
    label: "Theme settings",
    description: "Switch color theme",
    to: "/settings/theme",
  },
  {
    id: "settings.density",
    label: "Density settings",
    description: "Change row density",
    to: "/settings/density",
  },
  {
    id: "settings.keybindings",
    label: "Keybinding help",
    description: "View keyboard shortcuts",
    to: "/settings/keybindings",
  },
  {
    id: "settings.notifications",
    label: "Notification settings",
    description: "Configure browser alerts and VIPs",
    to: "/settings/notifications",
  },
  {
    id: "settings.compose",
    label: "Compose settings",
    description: "Choose editor preference",
    to: "/settings/compose",
  },
  {
    id: "settings.voice",
    label: "Voice settings",
    description: "Inspect local voice profile",
    to: "/settings/voice",
  },
  {
    id: "settings.llm",
    label: "LLM settings",
    description: "Configure summaries and draft assist",
    to: "/settings/llm",
  },
  {
    id: "settings.snippets",
    label: "Snippets",
    description: "Manage compose snippets",
    to: "/settings/snippets",
  },
  {
    id: "settings.token",
    label: "Bridge token",
    description: "Paste or inspect the bridge token",
    to: "/settings/token",
  },
];

export const settingsActions: Action[] = ROUTES.map(({ id, label, description, to }) => ({
  id,
  label,
  description,
  group: "Settings",
  icon: PaletteIcon,
  paletteOnly: true,
  run: () => {
    useModals.getState().setCommandPaletteOpen(false);
    getRuntimeNavigate().navigate(to);
  },
}));
