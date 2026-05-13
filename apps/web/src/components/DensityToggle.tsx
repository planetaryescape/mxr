import { Check, Rows2, Rows3, Rows4 } from "lucide-react";

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useUiPrefs, type Density } from "@/state/uiPrefsStore";

const options: { id: Density; label: string; Icon: typeof Rows2 }[] = [
  { id: "compact", label: "Compact", Icon: Rows4 },
  { id: "regular", label: "Regular", Icon: Rows3 },
  { id: "comfortable", label: "Comfortable", Icon: Rows2 },
];

export function DensityToggle() {
  const density = useUiPrefs((s) => s.density);
  const setDensity = useUiPrefs((s) => s.setDensity);
  const current = options.find((option) => option.id === density)?.label ?? "Regular";
  return (
    <ToggleGroup
      type="single"
      value={density}
      onValueChange={(value) => {
        if (value) setDensity(value as Density);
      }}
      aria-label="Mailbox density"
      className="pl-2"
    >
      <span className="w-20 text-xs font-medium text-foreground">{current}</span>
      {options.map(({ id, label, Icon }) => (
        <Tooltip key={id}>
          <TooltipTrigger asChild>
            <ToggleGroupItem
              value={id}
              size="icon"
              aria-label={`${label}${density === id ? " selected" : ""}`}
              className={cn(
                "relative",
                density === id &&
                  "bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/60",
              )}
            >
              <Icon className="size-3" />
              {density === id ? (
                <Check className="absolute right-0.5 top-0.5 size-2.5 rounded-full bg-primary-foreground/20" />
              ) : null}
            </ToggleGroupItem>
          </TooltipTrigger>
          <TooltipContent>{density === id ? `${label} selected` : label}</TooltipContent>
        </Tooltip>
      ))}
    </ToggleGroup>
  );
}
