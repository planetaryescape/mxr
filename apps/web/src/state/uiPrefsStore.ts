/*
 * Global UI prefs — theme, density, sidebar collapsed state, compose editor
 * choice. Persisted to localStorage and hydrated synchronously in main.tsx
 * before render so the theme doesn't flash.
 */

import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export type Theme = "midnight" | "light" | "eclipse" | "paper" | "system";
export type Density = "compact" | "regular" | "comfortable";
export type ComposeEditor = "codemirror-vim" | "tiptap";
export type EmailHtmlTheme = "dark" | "original";
export type ReaderLayout = "split" | "full";

export interface UiPrefsState {
  theme: Theme;
  density: Density;
  sidebarCollapsed: boolean;
  composeEditor: ComposeEditor;
  emailHtmlTheme: EmailHtmlTheme;
  readerLayout: ReaderLayout;
  notificationsEnabled: boolean;
  notifyAllNewMail: boolean;
  vipAllowlist: string[];
  setTheme: (t: Theme) => void;
  setDensity: (d: Density) => void;
  setSidebarCollapsed: (b: boolean) => void;
  setComposeEditor: (e: ComposeEditor) => void;
  setEmailHtmlTheme: (theme: EmailHtmlTheme) => void;
  setReaderLayout: (layout: ReaderLayout) => void;
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
      readerLayout: "split",
      notificationsEnabled: false,
      notifyAllNewMail: false,
      vipAllowlist: [],
      setTheme: (theme) => set({ theme }),
      setDensity: (density) => set({ density }),
      setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
      setComposeEditor: (composeEditor) => set({ composeEditor }),
      setEmailHtmlTheme: (emailHtmlTheme) => set({ emailHtmlTheme }),
      setReaderLayout: (readerLayout) => set({ readerLayout }),
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
      storage: createJSONStorage(() => uiPrefsStorage()),
      version: 2,
    },
  ),
);

const memoryPrefsStorage = new Map<string, string>();

function uiPrefsStorage(): StateStorage {
  try {
    if (typeof window !== "undefined" && window.localStorage) return window.localStorage;
  } catch {
    // Some test/webview environments expose `window` but disable localStorage.
  }
  return {
    getItem: (name) => memoryPrefsStorage.get(name) ?? null,
    setItem: (name, value) => {
      memoryPrefsStorage.set(name, value);
    },
    removeItem: (name) => {
      memoryPrefsStorage.delete(name);
    },
  };
}

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
