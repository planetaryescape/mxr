import { useMutation } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { BarChart3, RefreshCw } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { rebuildAnalytics, type AnalyticsRange } from "./api";
import { CadenceDashboard } from "./CadenceDashboard";
import { ContactsDashboard } from "./ContactsDashboard";
import { ResponseTimeDashboard } from "./ResponseTimeDashboard";
import { SearchGroupsDashboard } from "./SearchGroupsDashboard";
import { StaleDashboard } from "./StaleDashboard";
import { StorageDashboard } from "./StorageDashboard";
import { SubscriptionsDashboard } from "./SubscriptionsDashboard";
import { WrappedDashboard } from "./WrappedDashboard";
import { Page, PageTabs } from "@/components/Page";
import { PageEmpty } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type DashboardId =
  | "storage"
  | "stale"
  | "contacts"
  | "cadence"
  | "response-time"
  | "subscriptions"
  | "search-groups"
  | "wrapped";

/** Every TUI analytics view, in the TUI's order. `ranged` shows the range picker. */
const DASHBOARDS: { id: DashboardId; label: string; description: string; ranged: boolean }[] = [
  {
    id: "storage",
    label: "Storage",
    description: "What takes up space, and who sent it.",
    ranged: true,
  },
  { id: "stale", label: "Stale", description: "Conversations waiting on a reply.", ranged: true },
  {
    id: "contacts",
    label: "Contacts",
    description: "Lopsided and fading relationships.",
    ranged: false,
  },
  {
    id: "cadence",
    label: "Cadence",
    description: "Watched contacts who have gone quiet.",
    ranged: false,
  },
  {
    id: "response-time",
    label: "Response time",
    description: "How fast replies come and go.",
    ranged: true,
  },
  {
    id: "subscriptions",
    label: "Subscriptions",
    description: "Newsletters and how much you read them.",
    ranged: false,
  },
  {
    id: "search-groups",
    label: "Search groups",
    description: "Any search, counted by sender, list or category.",
    ranged: false,
  },
  {
    id: "wrapped",
    label: "Wrapped",
    description: "Your mail over a period, in one page.",
    ranged: true,
  },
];

const RANGES: { id: AnalyticsRange; label: string }[] = [
  { id: "7d", label: "7 days" },
  { id: "30d", label: "30 days" },
  { id: "90d", label: "90 days" },
  { id: "1y", label: "1 year" },
];

export function AnalyticsDashboardRoute() {
  const { dashboard } = useParams({ from: "/analytics/$dashboard" });
  const navigate = useNavigate();
  const [range, setRange] = useState<AnalyticsRange>("90d");
  const rebuild = useMutation({
    mutationFn: rebuildAnalytics,
    onSuccess: () => toast.success("Analytics rebuild queued"),
    onError: (error) => toast.error("Rebuild failed", { description: error.message }),
  });
  const current = DASHBOARDS.find((item) => item.id === dashboard);

  return (
    <Page
      eyebrow="Analytics"
      title={current?.label ?? "Analytics"}
      description={current?.description}
      width="full"
      actions={
        <>
          {current?.ranged ? (
            <Select value={range} onValueChange={(value) => setRange(value as AnalyticsRange)}>
              <SelectTrigger className="h-8 w-28 text-xs" aria-label="Time range">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RANGES.map((item) => (
                  <SelectItem key={item.id} value={item.id} className="text-xs">
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : null}
          <Button
            variant="outline"
            size="sm"
            onClick={() => rebuild.mutate()}
            disabled={rebuild.isPending}
          >
            <RefreshCw className="size-3" />
            Rebuild
          </Button>
        </>
      }
      tabs={
        <PageTabs
          label="Analytics dashboards"
          value={(current?.id ?? "storage") as DashboardId}
          tabs={DASHBOARDS.map(({ id, label }) => ({ id, label }))}
          onChange={(id) =>
            void navigate({ to: "/analytics/$dashboard", params: { dashboard: id } })
          }
        />
      }
    >
      <div className="mx-auto max-w-[84rem]">
        <Dashboard dashboard={current?.id} requested={dashboard} range={range} />
      </div>
    </Page>
  );
}

function Dashboard({
  dashboard,
  requested,
  range,
}: {
  dashboard: DashboardId | undefined;
  requested: string;
  range: AnalyticsRange;
}) {
  switch (dashboard) {
    case "storage":
      return <StorageDashboard range={range} />;
    case "stale":
      return <StaleDashboard range={range} />;
    case "contacts":
      return <ContactsDashboard />;
    case "cadence":
      return <CadenceDashboard />;
    case "response-time":
      return <ResponseTimeDashboard range={range} />;
    case "subscriptions":
      return <SubscriptionsDashboard />;
    case "search-groups":
      return <SearchGroupsDashboard />;
    case "wrapped":
      return <WrappedDashboard range={range} />;
    case undefined:
      return (
        <PageEmpty
          icon={<BarChart3 className="size-5" />}
          title="No such dashboard"
          body={`"${requested}" is not an analytics view. Pick one of the tabs above.`}
        />
      );
  }
}
