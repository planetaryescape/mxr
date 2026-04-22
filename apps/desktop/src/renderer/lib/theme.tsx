import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type {
  DesktopSettings,
  DesktopSettingsPatch,
  DesktopThemeId,
} from "../../shared/types";

const DEFAULT_SETTINGS: DesktopSettings = {
  theme: "mxr-dark",
  keymapOverrides: {},
};

interface DesktopSettingsContextValue {
  loaded: boolean;
  settings: DesktopSettings;
  theme: DesktopThemeId;
  setTheme: (theme: DesktopThemeId) => Promise<DesktopSettings>;
  updateDesktopSettings: (
    patch: DesktopSettingsPatch,
  ) => Promise<DesktopSettings>;
}

const DesktopSettingsContext = createContext<DesktopSettingsContextValue>({
  loaded: false,
  settings: DEFAULT_SETTINGS,
  theme: DEFAULT_SETTINGS.theme,
  setTheme: async () => DEFAULT_SETTINGS,
  updateDesktopSettings: async () => DEFAULT_SETTINGS,
});

export function useTheme() {
  const context = useContext(DesktopSettingsContext);
  return {
    loaded: context.loaded,
    theme: context.theme,
    setTheme: context.setTheme,
  };
}

export function useDesktopSettings() {
  return useContext(DesktopSettingsContext);
}

export function ThemeProvider(props: { children: ReactNode }) {
  const [loaded, setLoaded] = useState(false);
  const [settings, setSettings] = useState<DesktopSettings>(DEFAULT_SETTINGS);

  useEffect(() => {
    let cancelled = false;

    void window.mxrDesktop
      .getDesktopSettings()
      .then((next) => {
        if (cancelled) {
          return;
        }
        setSettings(next);
        setLoaded(true);
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setLoaded(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", settings.theme);
  }, [settings.theme]);

  const value = useMemo<DesktopSettingsContextValue>(
    () => ({
      loaded,
      settings,
      theme: settings.theme,
      setTheme: async (theme) => {
        const next = await window.mxrDesktop.updateDesktopSettings({ theme });
        setSettings(next);
        return next;
      },
      updateDesktopSettings: async (patch) => {
        const next = await window.mxrDesktop.updateDesktopSettings(patch);
        setSettings(next);
        return next;
      },
    }),
    [loaded, settings],
  );

  return (
    <DesktopSettingsContext value={value}>
      {props.children}
    </DesktopSettingsContext>
  );
}
