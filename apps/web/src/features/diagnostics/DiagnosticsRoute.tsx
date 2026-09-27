import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Bug } from "lucide-react";
import { toast } from "sonner";

import {
  backfillSemantic,
  fetchAdminStatus,
  fetchBugReport,
  fetchDiagnostics,
  fetchSemanticStatus,
  installSemanticProfile,
  reindexSemantic,
  semanticSnapshot,
  setSemanticEnabled,
  useSemanticProfile,
} from "./api";
import { EventsPanel } from "./EventsPanel";
import { LogsPanel } from "./LogsPanel";
import { OverviewPanel } from "./OverviewPanel";
import { SemanticPanel } from "./SemanticPanel";
import { Page, PageTabs } from "@/components/Page";
import { Button } from "@/components/ui/button";
import { ActivityBrowser } from "@/features/activity/ActivityRoute";

export { DiagnosticValue } from "./DiagnosticValue";

export type DiagnosticsPanel = "overview" | "logs" | "events" | "activity";

export function DiagnosticsRoute() {
  const search = useSearch({ from: "/diagnostics" });
  const navigate = useNavigate();
  const status = useQuery({
    queryKey: ["diagnostics", "status"],
    queryFn: fetchAdminStatus,
    refetchInterval: 10_000,
  });
  const doctor = useQuery({ queryKey: ["diagnostics", "doctor"], queryFn: fetchDiagnostics });
  const semantic = useQuery({
    queryKey: ["diagnostics", "semantic"],
    queryFn: fetchSemanticStatus,
    refetchInterval: 15_000,
  });
  const semanticStatus = semanticSnapshot(semantic.data);
  const bug = useMutation({
    mutationFn: async () => {
      const result = await fetchBugReport();
      await navigator.clipboard.writeText(result.content);
    },
    onSuccess: () => toast.success("Bug report copied", { description: "Paste it into an issue." }),
    onError: (error) => toast.error("Bug report failed", { description: error.message }),
  });
  const semanticBackfill = useMutation({
    mutationFn: backfillSemantic,
    onSuccess: () => {
      toast.success("Semantic backfill queued");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic backfill failed", { description: error.message }),
  });
  const semanticEnable = useMutation({
    mutationFn: setSemanticEnabled,
    onSuccess: (_, enabled) => {
      toast.success(enabled ? "Semantic search enabled" : "Semantic search disabled");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic update failed", { description: error.message }),
  });
  const semanticReindex = useMutation({
    mutationFn: reindexSemantic,
    onSuccess: () => {
      toast.success("Semantic reindex queued");
      refreshSemanticHealth();
    },
    onError: (error) => toast.error("Semantic reindex failed", { description: error.message }),
  });
  const semanticInstall = useMutation({
    mutationFn: installSemanticProfile,
    onSuccess: (_, profile) => {
      toast.success(`${profile} install queued`);
      refreshSemanticHealth();
    },
    onError: (error) =>
      toast.error("Semantic profile install failed", { description: error.message }),
  });
  const semanticUse = useMutation({
    mutationFn: useSemanticProfile,
    onSuccess: (_, profile) => {
      toast.success(`${profile} selected`);
      refreshSemanticHealth();
    },
    onError: (error) =>
      toast.error("Semantic profile switch failed", { description: error.message }),
  });

  function refreshSemanticHealth() {
    void semantic.refetch();
    void doctor.refetch();
    void status.refetch();
  }

  const panel = search.panel ?? "overview";
  return (
    <Page
      title="Diagnostics"
      description="Daemon health, sync, logs, events and semantic search."
      width="full"
      actions={
        <Button variant="outline" size="sm" onClick={() => bug.mutate()} disabled={bug.isPending}>
          <Bug className="size-3" />
          Copy bug report
        </Button>
      }
      tabs={
        <PageTabs
          label="Diagnostics panels"
          value={panel}
          onChange={(next) =>
            void navigate({
              to: "/diagnostics",
              search: next === "overview" ? {} : { panel: next },
              replace: true,
            })
          }
          tabs={PANELS}
        />
      }
    >
      <div className="mx-auto max-w-[84rem]">
        {panel === "overview" ? (
          <>
            <OverviewPanel status={status} doctor={doctor} />
            <SemanticPanel
              status={semanticStatus}
              loading={semantic.isLoading}
              enablePending={semanticEnable.isPending}
              backfillPending={semanticBackfill.isPending}
              reindexPending={semanticReindex.isPending}
              installPending={semanticInstall.isPending}
              usePending={semanticUse.isPending}
              onSetEnabled={(enabled) => semanticEnable.mutate(enabled)}
              onBackfill={() => semanticBackfill.mutate()}
              onReindex={() => semanticReindex.mutate()}
              onInstall={(profile) => semanticInstall.mutate(profile)}
              onUse={(profile) => semanticUse.mutate(profile)}
            />
          </>
        ) : panel === "logs" ? (
          <LogsPanel />
        ) : panel === "events" ? (
          <EventsPanel />
        ) : (
          <>
            <p className="mb-4 max-w-[65ch] text-[12.5px] text-muted-foreground">
              A local record of what you did in the TUI, CLI and web. It never leaves this machine.
            </p>
            <ActivityBrowser embedded />
          </>
        )}
      </div>
    </Page>
  );
}

const PANELS: { id: DiagnosticsPanel; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "logs", label: "Logs" },
  { id: "events", label: "Events" },
  { id: "activity", label: "Activity" },
];
