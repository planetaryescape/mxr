import { Moon, Palette, Sun } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useUiPrefs, type Theme } from "@/state/uiPrefsStore";

const themes: { id: Theme; label: string; description: string }[] = [
  { id: "midnight", label: "Midnight", description: "Default dark, sky cyan accent" },
  { id: "eclipse", label: "Eclipse", description: "Cooler dark, magenta accent" },
  { id: "paper", label: "Paper", description: "Warm off-white, low contrast" },
  { id: "light", label: "Light", description: "Bright, high contrast" },
  { id: "system", label: "System", description: "Match OS preference" },
];

export function ThemePicker() {
  const theme = useUiPrefs((s) => s.theme);
  const setTheme = useUiPrefs((s) => s.setTheme);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="Theme">
          {theme === "light" || theme === "paper" ? (
            <Sun className="size-3.5" />
          ) : (
            <Moon className="size-3.5" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuLabel>Theme</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as Theme)}>
          {themes.map((t) => (
            <DropdownMenuRadioItem
              key={t.id}
              value={t.id}
              className="flex flex-col items-start py-2"
            >
              <span className="text-xs font-medium">{t.label}</span>
              <span className="text-2xs text-muted-foreground">{t.description}</span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled className="text-2xs text-muted-foreground">
          <Palette className="size-3" /> More in /settings/theme
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
