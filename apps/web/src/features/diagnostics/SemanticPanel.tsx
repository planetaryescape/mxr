import { Button } from "@/components/ui/button";
import { PageSection } from "@/components/Page";
import { FactList, PageSkeleton } from "@/components/PageParts";

import { semanticProfiles, type SemanticProfile, type SemanticStatusSnapshot } from "./api";

export function SemanticPanel({
  status,
  loading,
  enablePending,
  backfillPending,
  reindexPending,
  installPending,
  usePending,
  onSetEnabled,
  onBackfill,
  onReindex,
  onInstall,
  onUse,
}: {
  status: SemanticStatusSnapshot | null;
  loading: boolean;
  enablePending: boolean;
  backfillPending: boolean;
  reindexPending: boolean;
  installPending: boolean;
  usePending: boolean;
  onSetEnabled: (enabled: boolean) => void;
  onBackfill: () => void;
  onReindex: () => void;
  onInstall: (profile: SemanticProfile) => void;
  onUse: (profile: SemanticProfile) => void;
}) {
  const profiles = new Map((status?.profiles ?? []).map((record) => [record.profile, record]));
  const busy = enablePending || backfillPending || reindexPending || installPending || usePending;
  return (
    <PageSection
      title="Semantic controls"
      description="Local embeddings behind semantic and hybrid search."
      actions={
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => onSetEnabled(!(status?.enabled ?? false))}
            disabled={!status || enablePending}
          >
            {status?.enabled ? "Disable semantic" : "Enable semantic"}
          </Button>
          <Button variant="outline" size="sm" onClick={onBackfill} disabled={backfillPending}>
            Backfill semantic
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onReindex}
            disabled={!status || reindexPending}
          >
            Reindex active profile
          </Button>
        </div>
      }
    >
      {!status ? (
        loading ? (
          <PageSkeleton rows={3} label="Loading semantic status" />
        ) : (
          <p className="text-[13px] text-muted-foreground">
            The daemon reported no semantic status.
          </p>
        )
      ) : (
        <div className="space-y-3">
          <FactList
            facts={[
              ["Enabled", status.enabled ? "yes" : "no"],
              ["Active profile", status.active_profile],
              ["Queue", String(status.runtime?.queue_depth ?? 0)],
              ["In flight", String(status.runtime?.in_flight ?? 0)],
            ]}
          />
          <div>
            {semanticProfiles.map((profile) => {
              const record = profiles.get(profile);
              const active = status.active_profile === profile;
              return (
                <div
                  key={profile}
                  className="grid gap-2 border-b border-border/60 py-2.5 md:grid-cols-[1fr_auto] md:items-center"
                >
                  <div>
                    <div className="font-mono text-xs text-foreground">{profile}</div>
                    <div className="mt-1 text-2xs text-muted-foreground">
                      {record
                        ? `${record.status} · ${record.backend} · ${record.dimensions} dims · ${record.progress_completed.toLocaleString()} of ${record.progress_total.toLocaleString()} indexed`
                        : "Not installed"}
                    </div>
                    {record?.last_error ? (
                      <div className="mt-1 text-2xs text-destructive">{record.last_error}</div>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => onInstall(profile)}
                      disabled={busy}
                    >
                      Install
                    </Button>
                    <Button
                      variant={active ? "secondary" : "outline"}
                      size="sm"
                      onClick={() => onUse(profile)}
                      disabled={busy || active || !record}
                    >
                      {active ? "Active" : "Use"}
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </PageSection>
  );
}
