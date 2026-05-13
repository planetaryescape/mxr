import { useRouterState } from "@tanstack/react-router";
import { Pencil } from "lucide-react";

import { DensityToggle } from "@/components/DensityToggle";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/features/search/SearchInput";
import { useModals } from "@/state/modalStore";

export function Topbar() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const setComposeOpen = useModals((state) => state.setComposeLauncherOpen);

  return (
    <div className="flex w-full items-center gap-3">
      <Breadcrumb path={path} />

      <SearchInput />

      <DensityToggle />

      <Button size="sm" onClick={() => setComposeOpen(true)} aria-label="Compose new email">
        <Pencil className="size-3" />
        Compose
      </Button>
    </div>
  );
}

function Breadcrumb({ path }: { path: string }) {
  const parts = path.split("/").filter(Boolean);
  if (parts.length === 0) return <div className="font-mono text-2xs text-muted-foreground">/</div>;
  return (
    <div className="flex items-center gap-1 truncate font-mono text-2xs text-muted-foreground">
      <span>/</span>
      {parts.map((part, i) => (
        // eslint-disable-next-line react/no-array-index-key
        <span key={i} className="flex items-center gap-1">
          <span className={i === parts.length - 1 ? "text-foreground" : undefined}>
            {decodeURIComponent(part)}
          </span>
          {i < parts.length - 1 && <span>/</span>}
        </span>
      ))}
    </div>
  );
}
