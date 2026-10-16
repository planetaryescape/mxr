/*
 * Global UI prefs — theme, density, sidebar collapsed state, compose editor
 * choice. Persisted to localStorage and hydrated synchronously in main.tsx
 * before render so the theme doesn't flash.
 */

import { parseReaderSettings, type ReaderSettings } from "@planetaryescape/reader";
import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export type Theme = "midnight" | "light" | "eclipse" | "paper" | "system";
export type Density = "compact" | "regular" | "comfortable";
/** "system" follows the OS reduced-motion preference. */
export type MotionPref = "system" | "reduced" | "full";
export type ContrastPref = "system" | "standard" | "increased";
export type ComposeEditor = "codemirror-vim" | "tiptap";
export type EmailHtmlTheme = "auto" | "dark" | "original";
export type ExplicitEmailHtmlTheme = Exclude<EmailHtmlTheme, "auto">;
export interface EmailHtmlSenderOverride {
  sender: string;
  theme: ExplicitEmailHtmlTheme;
}
export type ReaderLayout = "split" | "full";
export type ReaderView = "formatted" | "reader" | "plain";
/** Undo-send window in seconds; 0 sends immediately. */
export type UndoSendSeconds = 0 | 5 | 10 | 30;
/** Where `/` lands: what needs you, or what arrived. */
export type HomeView = "now" | "inbox";

/** How the Reading reader draws an article: the reader theme, typeface, size and line spacing. */
export type ReadingDisplay = Pick<ReaderSettings, "theme" | "typeface" | "size" | "spacing">;

export const DEFAULT_READING_DISPLAY: ReadingDisplay = {
  theme: "auto",
  typeface: "serif",
  size: 20,
  spacing: "even",
};

/** Stored prefs may be hand-edited or from another version; repair them field by field. */
export function readReadingDisplay(value: unknown): ReadingDisplay {
  const saved = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  const { theme, typeface, size, spacing } = parseReaderSettings({
    ...DEFAULT_READING_DISPLAY,
    ...saved,
  });
  return { theme, typeface, size, spacing };
}

export interface UiPrefsState {
  home: HomeView;
  theme: Theme;
  density: Density;
  motion: MotionPref;
  contrast: ContrastPref;
  sidebarCollapsed: boolean;
  composeEditor: ComposeEditor;
  emailHtmlTheme: EmailHtmlTheme;
  emailHtmlSenderOverrides: EmailHtmlSenderOverride[];
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
  /** Reading reader appearance; a client preference like the theme, not daemon state. */
  readingDisplay: ReadingDisplay;
  setReadingDisplay: (patch: Partial<ReadingDisplay>) => void;
  setHome: (home: HomeView) => void;
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
  setContrast: (contrast: ContrastPref) => void;
  setSidebarCollapsed: (b: boolean) => void;
  setComposeEditor: (e: ComposeEditor) => void;
  setEmailHtmlTheme: (theme: EmailHtmlTheme) => void;
  setEmailHtmlThemeForSender: (sender: string, theme: ExplicitEmailHtmlTheme) => void;
  forgetEmailHtmlThemeForSender: (sender: string) => void;
  setReaderLayout: (layout: ReaderLayout) => void;
  setNotificationsEnabled: (b: boolean) => void;
  setNotifyAllNewMail: (b: boolean) => void;
  addVip: (pattern: string) => void;
  removeVip: (pattern: string) => void;
}

export const useUiPrefs = create<UiPrefsState>()(
  persist(
    (set) => ({
      home: "now",
      // Follow the OS appearance until someone picks a theme.
      theme: "system",
      density: "regular",
      motion: "system",
      contrast: "system",
      sidebarCollapsed: false,
      // Locked product decision (docs/web-app.md): CodeMirror + vim is the
      // default. Only the default changes; a persisted choice is kept as is.
      composeEditor: "codemirror-vim",
      emailHtmlTheme: "auto",
      emailHtmlSenderOverrides: [],
      readerLayout: "split",
      notificationsEnabled: false,
      notifyAllNewMail: false,
      vipAllowlist: [],
      undoSendSeconds: 10,
      accountScope: null,
      // The rail leads with the modes and Inbox; its groups start folded.
      collapsedSections: ["mail", "views", "tools", "labels"],
      listMode: "threads",
      readerView: "formatted",
      remoteImageSenders: [],
      readingDisplay: DEFAULT_READING_DISPLAY,
      setReadingDisplay: (patch) =>
        set((s) => ({ readingDisplay: readReadingDisplay({ ...s.readingDisplay, ...patch }) })),
      setHome: (home) => set({ home }),
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
      setContrast: (contrast) => set({ contrast }),
      setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
      setComposeEditor: (composeEditor) => set({ composeEditor }),
      setEmailHtmlTheme: (emailHtmlTheme) => set({ emailHtmlTheme }),
      setEmailHtmlThemeForSender: (sender, theme) =>
        set((state) => {
          const normalizedSender = normalizeSender(sender);
          if (!normalizedSender) return state;
          return {
            emailHtmlSenderOverrides: [
              ...readEmailHtmlSenderOverrides(state.emailHtmlSenderOverrides).filter(
                (override) => override.sender !== normalizedSender,
              ),
              { sender: normalizedSender, theme },
            ],
          };
        }),
      forgetEmailHtmlThemeForSender: (sender) =>
        set((state) => {
          const normalizedSender = normalizeSender(sender);
          if (!normalizedSender) return state;
          return {
            emailHtmlSenderOverrides: readEmailHtmlSenderOverrides(
              state.emailHtmlSenderOverrides,
            ).filter((override) => override.sender !== normalizedSender),
          };
        }),
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
      version: 7,
      merge: (persisted, current) => mergeUiPrefs(persisted, current),
      // persist merges what migrate returns over the defaults, so a partial
      // state is what it expects at runtime; its type says the whole store.
      migrate: (persisted, version) => migrateUiPrefs(persisted, version) as UiPrefsState,
    },
  ),
);

/**
 * v3 folds the sidebar's new "More" and "Labels" groups for people who
 * already had saved prefs, so the sidebar opens on places for everyone.
 * v4: Now replaced the desk as the front page, so a saved "desk" home is
 * Now. v5 adds sender-specific HTML canvas preferences; previous explicit
 * Dark and Original choices stay as they were. v6 adds system/default/increased
 * contrast, defaulting existing users to system. v7 splits the rail's "More"
 * into Mail and Views: a folded More folds both.
 */
export function migrateUiPrefs(persisted: unknown, version: number): Partial<UiPrefsState> {
  let state: Partial<UiPrefsState> =
    persisted && typeof persisted === "object" ? (persisted as Partial<UiPrefsState>) : {};
  if (version < 3) {
    const folded = Array.isArray(state.collapsedSections) ? state.collapsedSections : ["tools"];
    state = { ...state, collapsedSections: [...new Set([...folded, "more", "labels"])] };
  }
  if (version < 4 && state.home !== "inbox") state = { ...state, home: "now" };
  if (version < 6) state = { ...state, contrast: "system" };
  if (version < 7 && Array.isArray(state.collapsedSections)) {
    const sections = state.collapsedSections;
    // A folded More folds both of its successors; an open one leaves them open.
    if (sections.includes("more")) {
      state = {
        ...state,
        collapsedSections: [
          ...new Set([...sections.filter((id) => id !== "more"), "mail", "views"]),
        ],
      };
    }
  }
  let migrated = state;
  if ("emailHtmlTheme" in migrated && !isEmailHtmlTheme(migrated.emailHtmlTheme)) {
    migrated = { ...migrated };
    delete migrated.emailHtmlTheme;
  }
  if ("emailHtmlSenderOverrides" in migrated) {
    migrated = {
      ...migrated,
      emailHtmlSenderOverrides: readEmailHtmlSenderOverrides(migrated.emailHtmlSenderOverrides),
    };
  }
  return migrated;
}

export function normalizeSender(sender: string): string {
  return sender.trim().toLowerCase();
}

export function readEmailHtmlSenderOverrides(value: unknown): EmailHtmlSenderOverride[] {
  if (!Array.isArray(value)) return [];
  const overrides = new Map<string, ExplicitEmailHtmlTheme>();
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const candidate = item as { sender?: unknown; theme?: unknown };
    if (typeof candidate.sender !== "string" || !isExplicitEmailHtmlTheme(candidate.theme))
      continue;
    const sender = normalizeSender(candidate.sender);
    if (sender) overrides.set(sender, candidate.theme);
  }
  return [...overrides].map(([sender, theme]) => ({ sender, theme }));
}

export function emailHtmlThemeForSender(
  overrides: unknown,
  sender: string | null,
): ExplicitEmailHtmlTheme | undefined {
  if (!sender) return undefined;
  const normalizedSender = normalizeSender(sender);
  return readEmailHtmlSenderOverrides(overrides).find(
    (override) => override.sender === normalizedSender,
  )?.theme;
}

function mergeUiPrefs(persisted: unknown, current: UiPrefsState): UiPrefsState {
  const saved = persisted && typeof persisted === "object" ? persisted : {};
  const state = saved as Partial<UiPrefsState>;
  return {
    ...current,
    ...state,
    contrast: isContrastPref(state.contrast) ? state.contrast : current.contrast,
    emailHtmlTheme: isEmailHtmlTheme(state.emailHtmlTheme)
      ? state.emailHtmlTheme
      : current.emailHtmlTheme,
    emailHtmlSenderOverrides: readEmailHtmlSenderOverrides(state.emailHtmlSenderOverrides),
    readingDisplay: readReadingDisplay(state.readingDisplay),
  };
}

function isContrastPref(value: unknown): value is ContrastPref {
  return value === "system" || value === "standard" || value === "increased";
}

function isEmailHtmlTheme(value: unknown): value is EmailHtmlTheme {
  return value === "auto" || isExplicitEmailHtmlTheme(value);
}

function isExplicitEmailHtmlTheme(value: unknown): value is ExplicitEmailHtmlTheme {
  return value === "dark" || value === "original";
}

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

const MORE_CONTRAST_QUERY = "(prefers-contrast: more)";

export function resolveContrast(contrast: ContrastPref): "standard" | "increased" {
  if (contrast !== "system") return contrast;
  if (typeof window === "undefined" || !window.matchMedia) return "standard";
  return window.matchMedia(MORE_CONTRAST_QUERY).matches ? "increased" : "standard";
}

export function applyContrastAttribute(contrast: ContrastPref): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-contrast", resolveContrast(contrast));
}

/** Re-resolve system contrast when the OS preference changes. */
export function watchSystemContrast(): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const media = window.matchMedia(MORE_CONTRAST_QUERY);
  const onChange = () => {
    const { contrast } = useUiPrefs.getState();
    if (contrast === "system") applyContrastAttribute(contrast);
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
