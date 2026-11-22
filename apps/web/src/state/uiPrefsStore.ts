import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export type Theme = "midnight" | "light" | "eclipse" | "paper" | "system";
export type Density = "compact" | "regular" | "comfortable";
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
export type UndoSendSeconds = 0 | 5 | 10 | 30;
export type HomeView = "now" | "inbox";

export type ReaderTheme = "auto" | "paper" | "sepia" | "snow" | "dusk" | "night" | "ink";
export type ReaderTypeface = "serif" | "book" | "sans" | "legible";
export type ReaderSpacing = "snug" | "even" | "airy";

export interface ReadingDisplay {
  theme: ReaderTheme;
  typeface: ReaderTypeface;
  size: number;
  spacing: ReaderSpacing;
}

export const DEFAULT_READING_DISPLAY: ReadingDisplay = {
  theme: "auto",
  typeface: "serif",
  size: 20,
  spacing: "even",
};

const READER_THEMES: readonly ReaderTheme[] = [
  "auto",
  "paper",
  "sepia",
  "snow",
  "dusk",
  "night",
  "ink",
];
const READER_TYPEFACES: readonly ReaderTypeface[] = ["serif", "book", "sans", "legible"];
const READER_SPACING: readonly ReaderSpacing[] = ["snug", "even", "airy"];

export function readReadingDisplay(value: unknown): ReadingDisplay {
  const saved = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  const field = (key: keyof ReadingDisplay) => (saved as Record<string, unknown>)[key];
  const pick = <T extends string>(
    options: readonly T[],
    key: keyof ReadingDisplay,
    fallback: T,
  ): T => {
    const raw = field(key);
    return options.find((option) => option === raw) ?? fallback;
  };
  const size = field("size");
  return {
    theme: pick(READER_THEMES, "theme", DEFAULT_READING_DISPLAY.theme),
    typeface: pick(READER_TYPEFACES, "typeface", DEFAULT_READING_DISPLAY.typeface),
    size:
      typeof size === "number" && Number.isFinite(size) && size >= 14 && size <= 28
        ? size
        : DEFAULT_READING_DISPLAY.size,
    spacing: pick(READER_SPACING, "spacing", DEFAULT_READING_DISPLAY.spacing),
  };
}

export interface UiPrefsState {
  home: HomeView;
  theme: Theme;
  density: Density;
  motion: MotionPref;
  contrast: ContrastPref;
  sidebarCollapsed: boolean;
  composeEditor: ComposeEditor;
  composeEditorByDraft: Record<string, ComposeEditor>;
  emailHtmlTheme: EmailHtmlTheme;
  emailHtmlSenderOverrides: EmailHtmlSenderOverride[];
  readerLayout: ReaderLayout;
  notificationsEnabled: boolean;
  notifyAllNewMail: boolean;
  vipAllowlist: string[];
  undoSendSeconds: UndoSendSeconds;
  accountScope: string | null;
  collapsedSections: string[];
  listMode: "threads" | "messages";
  readerView: ReaderView;
  remoteImageSenders: string[];
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
  setComposeEditorForDraft: (draftKey: string, e: ComposeEditor) => void;
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
      theme: "system",
      density: "regular",
      motion: "system",
      contrast: "system",
      sidebarCollapsed: false,
      composeEditor: "codemirror-vim",
      composeEditorByDraft: {},
      emailHtmlTheme: "auto",
      emailHtmlSenderOverrides: [],
      readerLayout: "split",
      notificationsEnabled: false,
      notifyAllNewMail: false,
      vipAllowlist: [],
      undoSendSeconds: 10,
      accountScope: null,
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
      setComposeEditorForDraft: (draftKey, composeEditor) =>
        set((state) => ({
          composeEditorByDraft: { ...state.composeEditorByDraft, [draftKey]: composeEditor },
        })),
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
      migrate: (persisted, version) => migrateUiPrefs(persisted, version) as UiPrefsState,
    },
  ),
);

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
    composeEditorByDraft: readComposeEditorByDraft(state.composeEditorByDraft),
    readingDisplay: readReadingDisplay(state.readingDisplay),
  };
}

function readComposeEditorByDraft(
  value: Record<string, ComposeEditor> | undefined,
): Record<string, ComposeEditor> {
  if (!value || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter(
      ([key, editor]) => key.length > 0 && (editor === "codemirror-vim" || editor === "tiptap"),
    ),
  );
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
  } catch {}
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
  document.documentElement.setAttribute(
    "data-scheme",
    LIGHT_THEMES.has(resolved) ? "light" : "dark",
  );
}

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

export function applyMotionAttribute(motion: MotionPref): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-motion", resolveMotion(motion));
}

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
