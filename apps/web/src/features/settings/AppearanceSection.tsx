import { SelectSetting } from "./settingsParts";
import {
  useUiPrefs,
  type Density,
  type ContrastPref,
  type HomeView,
  type MotionPref,
  type Theme,
} from "@/state/uiPrefsStore";

const HOMES: { value: HomeView; label: string }[] = [
  { value: "now", label: "Now" },
  { value: "inbox", label: "Inbox" },
];

const THEMES: { value: Theme; label: string }[] = [
  { value: "system", label: "Match the system" },
  { value: "midnight", label: "Midnight (dark)" },
  { value: "eclipse", label: "Eclipse (dark)" },
  { value: "light", label: "Light" },
  { value: "paper", label: "Paper (light)" },
];

export const DENSITIES: { value: Density; label: string }[] = [
  { value: "compact", label: "Compact" },
  { value: "regular", label: "Regular" },
  { value: "comfortable", label: "Comfortable" },
];

const MOTION: { value: MotionPref; label: string }[] = [
  { value: "system", label: "Match the system" },
  { value: "reduced", label: "Reduced" },
  { value: "full", label: "Full" },
];

const CONTRAST: { value: ContrastPref; label: string }[] = [
  { value: "system", label: "Match the system" },
  { value: "standard", label: "Default" },
  { value: "increased", label: "Increased" },
];

export function AppearanceSection() {
  const theme = useUiPrefs((state) => state.theme);
  const setTheme = useUiPrefs((state) => state.setTheme);
  const density = useUiPrefs((state) => state.density);
  const setDensity = useUiPrefs((state) => state.setDensity);
  const motion = useUiPrefs((state) => state.motion);
  const setMotion = useUiPrefs((state) => state.setMotion);
  const contrast = useUiPrefs((state) => state.contrast);
  const setContrast = useUiPrefs((state) => state.setContrast);
  const home = useUiPrefs((state) => state.home);
  const setHome = useUiPrefs((state) => state.setHome);
  return (
    <div>
      <SelectSetting
        label="Home"
        description="Where mxr opens. Now shows the few things that need you; the inbox shows what arrived, newest first."
        value={home}
        options={HOMES}
        onChange={setHome}
      />
      <SelectSetting
        label="Theme"
        description="Match the system follows your OS light or dark setting as it changes."
        value={theme}
        options={THEMES}
        onChange={setTheme}
      />
      <SelectSetting
        label="Density"
        description="Row height in mail lists."
        value={density}
        options={DENSITIES}
        onChange={setDensity}
      />
      <SelectSetting
        label="Motion"
        description="Reduced keeps fades and drops movement. Match the system follows your OS setting."
        value={motion}
        options={MOTION}
        onChange={setMotion}
      />
      <SelectSetting
        label="Contrast"
        description="Use your system preference, or choose stronger text and control borders."
        value={contrast}
        options={CONTRAST}
        onChange={setContrast}
      />
    </div>
  );
}
