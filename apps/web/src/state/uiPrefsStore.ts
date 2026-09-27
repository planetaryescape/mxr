/*
 * Global UI prefs — theme, density, sidebar collapsed state, compose editor
 * choice. Persisted to localStorage and hydrated synchronously in main.tsx
 * before render so the theme doesn't flash.
 */

import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export type Theme = "midnight" | "light" | "eclipse" | "paper" | "system";
export type Density = "compact" | "regular" | "comfortable";
/** "system" follows the OS reduced-motion preference. */
export type MotionPref = "system" | "reduced" | "full";
export type ComposeEditor = "codemirror-vim" | "tiptap";
export type EmailHtmlTheme = "dark" | "original";
export type ReaderLayout = "split" | "full";
export type ReaderView = "formatted" | "reader" | "plain";
/** Undo-send window in seconds; 0 sends immediately. */
export type UndoSendSeconds = 0 | 5 | 10 | 30;

export interface UiPrefsState {
  theme: Theme;
  density: Density;
  motion: MotionPref;
  sidebarCollapsed: boolean;
  composeEditor: ComposeEditor;
  emailHtmlTheme: EmailHtmlTheme;
  readerLayout: ReaderLayout;
  notificationsEnabled: boolean;
  notifyAllNewMail: boolean;
  vipAllowlist: string[];
  undoSendSeconds: UndoSendSeconds;
  /** Account the mail views are scoped to; null shows every account. */
  accountScope: string | null;
  /** Sidebar sections the user folded. */
  collapsedSections: string[];
  /** Group the list by conversation (TUI thread mode) or show messages. */
  listMode: "threads" | "messages";
  /** Body view the reader opens with: sanitized HTML, cleaned text, or raw text. */
  readerView: ReaderView;
  /** Senders whose remote images load without asking. */
  remoteImageSenders: string[];
  setReaderView: (view: ReaderView) => void;
  allowRemoteImagesFrom: (sender: string) => void;
  forgetRemoteImagesFrom: (sender: string) => void;
  setAccountScope: (accountId: string | null) => void;
  toggleSection: (sectionId: string) => void;
  setSectionCollapsed: (sectionId: string, collapsed: boolean) => void;
  setListMode: (mode: "threads" | "messages") => void;
  setUndoSendSeconds: (seconds: UndoSendSeconds) => void;
  setTheme: (t: Theme) => void;
  setDensity: (d: Density) => void;
  setMotion: (motion: MotionPref) => void;
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
      motion: "system",
      sidebarCollapsed: false,
      // Locked product decision (docs/web-app.md): CodeMirror + vim is the
      // default. Only the default changes; a persisted choice is kept as is.
      composeEditor: "codemirror-vim",
      emailHtmlTheme: "dark",
      readerLayout: "split",
      notificationsEnabled: false,
      notifyAllNewMail: false,
      vipAllowlist: [],
      undoSendSeconds: 10,
      accountScope: null,
      collapsedSections: ["tools"],
      listMode: "threads",
      readerView: "formatted",
      remoteImageSenders: [],
      setReaderView: (readerView) => set({ readerView }),
      allowRemoteImagesFrom: (sender) =>
        set((s) => ({
          remoteImageSenders: s.remoteImageSenders.includes(sender.toLowerCase())
            ? s.remoteImageSenders
            : [...s.remoteImageSenders, sender.toLowerCase()],
        })),
      forgetRemoteImagesFrom: (sender) =>
        set((s) => ({
          remoteImageSenders: s.remoteImageSenders.filter(
            (entry) => entry !== sender.toLowerCase(),
          ),
        })),
      setAccountScope: (accountScope) => set({ accountScope }),
      toggleSection: (sectionId) =>
        set((s) => ({
          collapsedSections: s.collapsedSections.includes(sectionId)
            ? s.collapsedSections.filter((id) => id !== sectionId)
            : [...s.collapsedSections, sectionId],
        })),
      setSectionCollapsed: (sectionId, collapsed) =>
        set((s) => ({
          collapsedSections: collapsed
            ? [...new Set([...s.collapsedSections, sectionId])]
            : s.collapsedSections.filter((id) => id !== sectionId),
        })),
      setListMode: (listMode) => set({ listMode }),
      setUndoSendSeconds: (undoSendSeconds) => set({ undoSendSeconds }),
      setTheme: (theme) => set({ theme }),
      setDensity: (density) => set({ density }),
      setMotion: (motion) => set({ motion }),
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

const LIGHT_THEMES = new Set<Theme>(["light", "paper"]);

export function resolveTheme(theme: Theme): Exclude<Theme, "system"> {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "midnight";
}

export function applyThemeAttribute(theme: Theme): void {
  if (typeof document === "undefined") return;
  const resolved = resolveTheme(theme);
  document.documentElement.setAttribute("data-theme", resolved);
  // `dark:` utilities key off the scheme, so custom dark themes get them too.
  document.documentElement.setAttribute(
    "data-scheme",
    LIGHT_THEMES.has(resolved) ? "light" : "dark",
  );
}

/** Re-resolve the "system" theme when the OS appearance changes. */
export function watchSystemTheme(): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const media = window.matchMedia("(prefers-color-scheme: light)");
  const onChange = () => {
    const { theme } = useUiPrefs.getState();
    if (theme === "system") applyThemeAttribute(theme);
  };
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

export function applyDensityAttribute(density: Density): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-density", density);
}

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

export function resolveMotion(motion: MotionPref): "reduced" | "full" {
  if (motion !== "system") return motion;
  if (typeof window === "undefined" || !window.matchMedia) return "full";
  return window.matchMedia(REDUCED_MOTION_QUERY).matches ? "reduced" : "full";
}

/** `data-motion` drives the reduced-motion policy in base.css. */
export function applyMotionAttribute(motion: MotionPref): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-motion", resolveMotion(motion));
}

/** Re-resolve "system" motion when the OS preference changes. */
export function watchSystemMotion(): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const media = window.matchMedia(REDUCED_MOTION_QUERY);
  const onChange = () => {
    const { motion } = useUiPrefs.getState();
    if (motion === "system") applyMotionAttribute(motion);
  };
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
