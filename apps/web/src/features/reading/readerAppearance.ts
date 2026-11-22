import { readerRootProps, type ReaderRootProps, type ReaderTheme } from "@planetaryescape/reader";
import { useSyncExternalStore } from "react";

import { resolveTheme, useUiPrefs, type ReadingDisplay, type Theme } from "@/state/uiPrefsStore";

const READER_THEME_FOR_APP = {
  midnight: "ink",
  eclipse: "night",
  light: "snow",
  paper: "paper",
} satisfies Record<Exclude<Theme, "system">, Exclude<ReaderTheme, "auto">>;

const DARK_APP_THEMES = new Set<Theme>(["midnight", "eclipse"]);

export const READER_THEMES: { id: ReaderTheme; label: string }[] = [
  { id: "auto", label: "Match app" },
  { id: "paper", label: "Paper" },
  { id: "sepia", label: "Sepia" },
  { id: "snow", label: "Snow" },
  { id: "dusk", label: "Dusk" },
  { id: "night", label: "Night" },
  { id: "ink", label: "Ink" },
];

export function readingRootProps(
  display: ReadingDisplay,
  appTheme: Exclude<Theme, "system">,
): ReaderRootProps {
  const theme = display.theme === "auto" ? READER_THEME_FOR_APP[appTheme] : display.theme;
  return readerRootProps(
    { ...display, theme, pen: "cobalt", highlighter: "yellow", width: "even" },
    DARK_APP_THEMES.has(appTheme) ? "dark" : "light",
  );
}

const LIGHT_QUERY = "(prefers-color-scheme: light)";

function subscribeToScheme(onChange: () => void): () => void {
  const media = window.matchMedia(LIGHT_QUERY);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

export function useAppTheme(): Exclude<Theme, "system"> {
  const theme = useUiPrefs((state) => state.theme);
  useSyncExternalStore(
    subscribeToScheme,
    () => window.matchMedia(LIGHT_QUERY).matches,
    () => false,
  );
  return resolveTheme(theme);
}
