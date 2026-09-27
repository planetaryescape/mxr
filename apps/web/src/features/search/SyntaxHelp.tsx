/* Popover listing the search operators. */

import { HelpCircle } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { searchSyntaxRows } from "@/lib/searchSyntax";

export function SyntaxHelp() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-10"
          aria-label="Search operators"
        >
          <HelpCircle className="size-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <div className="mb-2 text-[13px] font-semibold">Operators</div>
        <div className="grid gap-1">
          {searchSyntaxRows.map(([operator, description]) => (
            <div key={operator} className="flex items-center justify-between gap-3 text-[12px]">
              <code className="rounded bg-muted px-1.5 py-0.5 font-mono">{operator}</code>
              <span className="text-muted-foreground">{description}</span>
            </div>
          ))}
        </div>
        <p className="mt-3 text-2xs text-muted-foreground">
          Press <KeyChip>/</KeyChip> anywhere for quick search.
        </p>
      </PopoverContent>
    </Popover>
  );
}
