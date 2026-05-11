import { Alert } from "@/components/ui/alert";
import { useConnectionStore } from "@/state/connectionStore";

export function SyncProgressBanner() {
  const sync = useConnectionStore((s) => s.syncProgress);
  if (!sync) return null;
  return (
    <Alert
      role="status"
      data-sync-banner
      className="rounded-none border-x-0 border-t-0 border-primary/30 bg-primary-muted px-4 py-2 text-xs text-foreground"
    >
      Syncing {sync.current} of {sync.total} messages
    </Alert>
  );
}
