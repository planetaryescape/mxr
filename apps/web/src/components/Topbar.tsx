import { useRouterState } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Pencil } from "lucide-react";

import { DensityToggle } from "@/components/DensityToggle";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/features/search/SearchInput";
import { fetchAdminStatus } from "@/features/diagnostics/api";
import { useModals } from "@/state/modalStore";

export function Topbar() {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const setComposeOpen = useModals((state) => state.setComposeLauncherOpen);

  // Surface a small chip whenever the bridge is bound to the demo profile.
  // Polled lazily; status is cheap and stable for the session.
  const { data: status } = useQuery({
    queryKey: ["admin-status-is-demo"],
    queryFn: fetchAdminStatus,
    staleTime: 60_000,
    refetchOnWindowFocus: false,
  });
  const isDemo = Boolean((status as { is_demo?: boolean } | undefined)?.is_demo);

  return (
    <div className="flex w-full items-center gap-3">
      <Breadcrumb path={path} />

      {isDemo ? <DemoChip /> : null}

      <SearchInput />

      <DensityToggle />

      <Button size="sm" onClick={() => setComposeOpen(true)} aria-label="Compose new email">
        <Pencil className="size-3" />
        Compose
      </Button>
    </div>
  );
}

function DemoChip() {
  return (
    <span
      className="rounded-sm bg-amber-300 px-1.5 py-0.5 font-mono text-2xs font-semibold text-amber-950"
      title="Demo profile — no real mail is being touched"
      aria-label="Demo mode active"
    >
      DEMO
    </span>
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
