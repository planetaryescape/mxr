/*
 * Read-only view of which model each AI feature uses, which providers a
 * Claude model goes through, and whether they are working. Keys are never
 * entered here: a secret does not travel over the bridge, so the panel says
 * which CLI command stores one.
 */

import { ArrowRight } from "lucide-react";

import type { components } from "@/api/generated";
import { Badge } from "@/components/ui/badge";

export type LlmFeatureStatus = components["schemas"]["LlmFeatureStatusData"];
export type LlmKeyStatus = components["schemas"]["LlmKeyStatusData"];
type LlmRouteStatus = components["schemas"]["LlmRouteStatusData"];

type BadgeVariant = "success" | "warning" | "destructive" | "secondary" | "outline";

const routeVariants: Record<LlmRouteStatus["state"], BadgeVariant> = {
  ok: "success",
  cooling: "warning",
  broken: "destructive",
  recovering: "outline",
  probing: "outline",
  no_key: "secondary",
};

export function LlmRoutesPanel({
  features,
  keys,
}: {
  features: LlmFeatureStatus[] | undefined;
  keys: LlmKeyStatus[] | undefined;
}) {
  const rows = features ?? [];
  const anyRouted = rows.some((feature) => feature.routed);
  if (rows.length === 0) return null;

  return (
    <section className="space-y-3" aria-label="Models and routes">
      <div>
        <h3 className="text-sm font-semibold">Models and routes</h3>
        <p className="mt-1 max-w-2xl text-2xs text-muted-foreground">
          What each feature uses right now. A Claude model goes to Anthropic first and to OpenRouter
          when Anthropic is not working; mxr remembers which one is failing and tries it again
          later.
        </p>
      </div>
      <ul className="divide-y divide-border/60 border-y border-border/60">
        {rows.map((feature) => (
          <li
            key={feature.feature}
            className="grid gap-1 py-2 lg:grid-cols-[180px_200px_1fr] lg:items-center"
          >
            <span className="text-xs font-medium">{featureLabel(feature.feature)}</span>
            <span className="font-mono text-2xs text-muted-foreground">{feature.model}</span>
            <FeatureRoutes feature={feature} />
          </li>
        ))}
      </ul>
      {anyRouted || keys?.some((key) => key.present) ? <KeyList keys={keys ?? []} /> : null}
      <p className="text-2xs text-muted-foreground">
        Keys are stored with the command line, not here:{" "}
        <code className="font-mono">printf %s &quot;$KEY&quot; | mxr llm key set anthropic</code>{" "}
        (or <code className="font-mono">openrouter</code>).
      </p>
    </section>
  );
}

function FeatureRoutes({ feature }: { feature: LlmFeatureStatus }) {
  if (!feature.enabled) {
    return <span className="text-2xs text-muted-foreground">Off</span>;
  }
  const blocked = feature.blocked ? (
    <span className="text-2xs text-warning">Blocked: {feature.blocked}</span>
  ) : null;
  if (!feature.routed) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-2xs text-muted-foreground">
          {feature.base_url ?? "this machine"}
        </span>
        <Badge variant="outline">{feature.cloud ? "cloud" : "local"}</Badge>
        {blocked}
      </div>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-2">
      {(feature.routes ?? []).map((route, index) => (
        <span key={route.provider} className="flex items-center gap-2">
          {index > 0 ? <ArrowRight aria-hidden className="size-3 text-muted-foreground" /> : null}
          <Badge variant={routeVariants[route.state] ?? "outline"}>
            {providerName(route.provider)}: {routeLabel(route)}
            {route.next ? " (in use)" : ""}
          </Badge>
        </span>
      ))}
      {blocked}
    </div>
  );
}

function KeyList({ keys }: { keys: LlmKeyStatus[] }) {
  return (
    <ul className="space-y-1 text-2xs text-muted-foreground" aria-label="Provider keys">
      {keys.map((key) => (
        <li key={key.provider}>
          <span className="font-medium text-foreground">{providerName(key.provider)}</span>
          {": "}
          {key.present
            ? key.source === "stored"
              ? "key stored"
              : `key from ${key.env_var}`
            : `no key (set ${key.env_var} or store one)`}
        </li>
      ))}
    </ul>
  );
}

function routeLabel(route: LlmRouteStatus): string {
  const reason = route.reason ? ` (${route.reason})` : "";
  switch (route.state) {
    case "ok":
      return "ok";
    case "no_key":
      return "no key";
    case "cooling":
      return `cooling until ${clockTime(route.retry_at)}${reason}`;
    case "broken":
      return `broken: ${route.reason ?? "needs attention"}`;
    case "recovering":
      return `retry due${reason}`;
    case "probing":
      return `being retried${reason}`;
    default:
      // A newer daemon may report a state this build doesn't know.
      return String(route.state);
  }
}

function clockTime(value: string | null | undefined): string {
  if (!value) return "soon";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "soon";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function providerName(provider: string): string {
  return provider === "openrouter"
    ? "OpenRouter"
    : provider === "anthropic"
      ? "Anthropic"
      : provider;
}

function featureLabel(key: string): string {
  const words = key.replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}
