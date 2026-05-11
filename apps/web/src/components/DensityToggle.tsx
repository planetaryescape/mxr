import { Rows2, Rows3, Rows4 } from "lucide-react";

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useUiPrefs, type Density } from "@/state/uiPrefsStore";

const options: { id: Density; label: string; Icon: typeof Rows2 }[] = [
  { id: "compact", label: "Compact", Icon: Rows4 },
  { id: "regular", label: "Regular", Icon: Rows3 },
  { id: "comfortable", label: "Comfortable", Icon: Rows2 },
];

export function DensityToggle() {
  const density = useUiPrefs((s) => s.density);
  const setDensity = useUiPrefs((s) => s.setDensity);
  return (
    <ToggleGroup
      type="single"
      value={density}
      onValueChange={(value) => {
        if (value) setDensity(value as Density);
      }}
      aria-label="Mailbox density"
    >
      {options.map(({ id, label, Icon }) => (
        <Tooltip key={id}>
          <TooltipTrigger asChild>
            <ToggleGroupItem
              value={id}
              size="icon"
              aria-label={`${label}${density === id ? " selected" : ""}`}
            >
              <Icon className="size-3" />
            </ToggleGroupItem>
          </TooltipTrigger>
          <TooltipContent>{density === id ? `${label} selected` : label}</TooltipContent>
        </Tooltip>
      ))}
    </ToggleGroup>
  );
}
