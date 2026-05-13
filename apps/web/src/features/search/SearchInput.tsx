import { Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useModals } from "@/state/modalStore";

export function SearchInput() {
  const setSearchOpen = useModals((s) => s.setSearchPaletteOpen);

  return (
    <Button
      type="button"
      variant="outline"
      className="ml-auto h-8 w-[340px] justify-start gap-2 px-3 text-left text-xs font-normal text-muted-foreground"
      onClick={() => setSearchOpen(true)}
      aria-label="Open mail search"
    >
      <Search className="size-3.5" />
      <span className="min-w-0 flex-1 truncate">Search mail</span>
      <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs text-muted-foreground">
        /
      </kbd>
    </Button>
  );
}
