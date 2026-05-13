/*
 * Global UI prefs — theme, density, sidebar collapsed state, compose editor
 * choice. Persisted to localStorage and hydrated synchronously in main.tsx
 * before render so the theme doesn't flash.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Theme = "midnight" | "light" | "eclipse" | "paper" | "system";
export type Density = "compact" | "regular" | "comfortable";
export type ComposeEditor = "codemirror-vim" | "tiptap";
export type EmailHtmlTheme = "dark" | "original";

export interface UiPrefsState {
  theme: Theme;
  density: Density;
  sidebarCollapsed: boolean;
  composeEditor: ComposeEditor;
  emailHtmlTheme: EmailHtmlTheme;
  notificationsEnabled: boolean;
  notifyAllNewMail: boolean;
  vipAllowlist: string[];
  setTheme: (t: Theme) => void;
  setDensity: (d: Density) => void;
  setSidebarCollapsed: (b: boolean) => void;
  setComposeEditor: (e: ComposeEditor) => void;
  setEmailHtmlTheme: (theme: EmailHtmlTheme) => void;
  setNotificationsEnabled: (b: boolean) => void;
  setNotifyAllNewMail: (b: boolean) => void;
  addVip: (pattern: string) => void;
  removeVip: (pattern: string) => void;
}

export const useUiPrefs = create<UiPrefsState>()(
  persist(
    (set) => ({
      theme: "midnight",
      density: "regular",
      sidebarCollapsed: false,
      composeEditor: "tiptap",
      emailHtmlTheme: "dark",
      notificationsEnabled: false,
      notifyAllNewMail: false,
      vipAllowlist: [],
      setTheme: (theme) => set({ theme }),
      setDensity: (density) => set({ density }),
      setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
      setComposeEditor: (composeEditor) => set({ composeEditor }),
      setEmailHtmlTheme: (emailHtmlTheme) => set({ emailHtmlTheme }),
      setNotificationsEnabled: (notificationsEnabled) => set({ notificationsEnabled }),
      setNotifyAllNewMail: (notifyAllNewMail) => set({ notifyAllNewMail }),
      addVip: (pattern) =>
        set((s) => ({
          vipAllowlist: s.vipAllowlist.includes(pattern)
            ? s.vipAllowlist
            : [...s.vipAllowlist, pattern],
        })),
      removeVip: (pattern) =>
        set((s) => ({ vipAllowlist: s.vipAllowlist.filter((p) => p !== pattern) })),
    }),
    {
      name: "mxr.uiPrefs",
      version: 2,
    },
  ),
);

export function applyThemeAttribute(theme: Theme): void {
  if (typeof document === "undefined") return;
  const resolved =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "midnight"
      : theme;
  document.documentElement.setAttribute("data-theme", resolved);
}

export function applyDensityAttribute(density: Density): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-density", density);
}
