/*
 * LLM provider settings: the base provider plus per-feature overrides.
 * mxr stores the name of an environment variable, never the key itself.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Field } from "./settingsParts";
import { apiFetch } from "@/api/client";
import { isGistQuery } from "@/features/thread/context/api";
import { PageSkeleton } from "@/components/PageParts";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

interface LlmConfig {
  enabled: boolean;
  base_url: string;
  model: string;
  api_key_env: string;
  context_window: number;
  request_timeout_secs: number;
  allow_cloud_relationship_data: boolean;
  overrides?: LlmOverrides | null;
}

type LlmOverrideKey = (typeof llmOverrideFeatures)[number]["key"];

interface LlmOverrideConfig {
  enabled?: boolean | null;
  base_url?: string | null;
  model?: string | null;
  api_key_env?: string | null;
  context_window?: number | null;
  request_timeout_secs?: number | null;
}

type LlmOverrides = Partial<Record<LlmOverrideKey, LlmOverrideConfig | null>>;

interface LlmStatus {
  enabled: boolean;
  provider: string;
  model: string;
  configured_model: string;
  base_url: string | null;
  api_key_env: string | null;
  api_key_present: boolean;
  context_window: number;
  supports_streaming: boolean;
  request_timeout_secs: number;
}

const defaultLlmConfig: LlmConfig = {
  enabled: false,
  base_url: "http://localhost:11434/v1",
  model: "qwen2.5:3b-instruct",
  api_key_env: "",
  context_window: 8192,
  request_timeout_secs: 120,
  allow_cloud_relationship_data: false,
  overrides: {},
};

const llmOverrideFeatures = [
  { key: "summarize", label: "Summaries" },
  { key: "relationship_summary", label: "Relationship summaries" },
  { key: "commitments", label: "Commitments" },
  { key: "draft_assist", label: "Draft assist" },
  { key: "draft_new", label: "Draft new" },
  { key: "draft_refine", label: "Draft refine" },
  { key: "voice_match", label: "Voice match" },
  { key: "humanize_rewrite", label: "Humanizer rewrite" },
] as const;

export function LlmSettingsSection() {
  const qc = useQueryClient();
  const config = useQuery({
    queryKey: ["llm-config"],
    queryFn: () => apiFetch<{ config: LlmConfig }>("/api/v1/platform/llm/config"),
  });
  const status = useQuery({
    queryKey: ["llm-status"],
    queryFn: () => apiFetch<{ status: LlmStatus }>("/api/v1/platform/llm/status"),
  });
  const [draft, setDraft] = useState<LlmConfig | null>(null);
  const currentStatus = status.data?.status;
  const configUnsupported = config.isError && isNotFoundError(config.error);

  useEffect(() => {
    if (config.data?.config) {
      setDraft(normalizeLlmConfig(config.data.config));
      return;
    }
    if (configUnsupported && currentStatus) {
      setDraft(llmConfigFromStatus(currentStatus));
    }
  }, [config.data?.config, configUnsupported, currentStatus]);

  const save = useMutation({
    mutationFn: (body: LlmConfig) =>
      apiFetch<{ config: LlmConfig }>("/api/v1/platform/llm/config", {
        method: "POST",
        body,
      }),
    onSuccess: (saved) => {
      setDraft(saved.config);
      toast.success("LLM config saved");
      void qc.invalidateQueries({ queryKey: ["llm-config"] });
      void qc.invalidateQueries({ queryKey: ["llm-status"] });
      // Gists written under the old model or privacy setting are stale.
      void qc.invalidateQueries({ predicate: isGistQuery });
    },
    onError: (error) =>
      toast.error(
        isNotFoundError(error)
          ? "This daemon does not support saving LLM config yet"
          : "Failed to save LLM config",
      ),
  });

  const isValid =
    draft !== null &&
    draft.base_url.trim().length > 0 &&
    draft.model.trim().length > 0 &&
    draft.context_window > 0 &&
    draft.request_timeout_secs > 0;

  return (
    <div className="space-y-6">
      <div className="space-y-4">
        <section className="border-b border-border/60 pb-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
                Thread summaries and draft assist
              </h2>
              <p className="mt-1 max-w-2xl text-xs text-muted-foreground">
                mxr stores an environment variable name, not the API key. Leave it empty for Ollama
                or LM Studio.
              </p>
            </div>
            {currentStatus ? (
              <Badge variant="outline" className="font-mono text-muted-foreground">
                provider: {currentStatus.provider}
              </Badge>
            ) : null}
          </div>
        </section>

        {configUnsupported ? (
          <Alert variant="warning" className="text-xs">
            The running daemon exposes LLM status, but not editable LLM config yet. Restart mxr
            daemon from this build or upgrade it to save changes.
          </Alert>
        ) : config.isError ? (
          <Alert variant="destructive" className="text-xs">
            Could not load LLM config.
          </Alert>
        ) : null}

        {draft ? (
          <section className="space-y-4">
            <div className="flex items-center justify-between border-b border-border/60 py-3">
              <div>
                <Label htmlFor="llm-enabled">Enable LLM features</Label>
                <p className="mt-1 text-2xs text-muted-foreground">
                  Summaries and draft assist use this provider after save.
                </p>
              </div>
              <Switch
                id="llm-enabled"
                checked={draft.enabled}
                onCheckedChange={(enabled) => setDraft({ ...draft, enabled })}
              />
            </div>
            <div className="flex items-center justify-between gap-4 border-b border-border/60 py-3">
              <div>
                <Label htmlFor="llm-allow-cloud-relationship-data">
                  Allow relationship data with cloud LLMs
                </Label>
                <p className="mt-1 text-2xs text-muted-foreground">
                  Required before relationship summaries, commitments, or voice-match checks use a
                  non-local endpoint.
                </p>
              </div>
              <Switch
                id="llm-allow-cloud-relationship-data"
                checked={draft.allow_cloud_relationship_data}
                onCheckedChange={(allow_cloud_relationship_data) =>
                  setDraft({ ...draft, allow_cloud_relationship_data })
                }
              />
            </div>

            <div className="grid gap-3 md:grid-cols-2">
              <Field label="Base URL">
                <Input
                  aria-label="Base URL"
                  value={draft.base_url}
                  onChange={(event) => setDraft({ ...draft, base_url: event.target.value })}
                  placeholder="http://localhost:11434/v1"
                />
              </Field>
              <Field label="Model">
                <Input
                  aria-label="Model"
                  value={draft.model}
                  onChange={(event) => setDraft({ ...draft, model: event.target.value })}
                  placeholder="qwen2.5:3b-instruct"
                />
              </Field>
              <Field label="API key environment variable">
                <Input
                  aria-label="API key environment variable"
                  value={draft.api_key_env}
                  onChange={(event) => setDraft({ ...draft, api_key_env: event.target.value })}
                  placeholder="OPENAI_API_KEY"
                />
              </Field>
              <Field label="Context window">
                <Input
                  aria-label="Context window"
                  type="number"
                  min={1}
                  value={draft.context_window}
                  onChange={(event) =>
                    setDraft({ ...draft, context_window: Number(event.target.value) })
                  }
                />
              </Field>
              <Field label="Request timeout seconds">
                <Input
                  aria-label="Request timeout"
                  type="number"
                  min={1}
                  value={draft.request_timeout_secs}
                  onChange={(event) =>
                    setDraft({ ...draft, request_timeout_secs: Number(event.target.value) })
                  }
                />
              </Field>
            </div>

            <div className="space-y-3 border-t border-border pt-4">
              <div>
                <h3 className="text-sm font-semibold">Feature overrides</h3>
                <p className="mt-1 text-2xs text-muted-foreground">
                  Leave fields blank to inherit the base provider. Use this for larger summary
                  models or warmer draft models without changing every LLM feature.
                </p>
              </div>
              <div className="space-y-2">
                {llmOverrideFeatures.map((feature) => {
                  const override = draft.overrides?.[feature.key] ?? null;
                  return (
                    <div
                      key={feature.key}
                      className="grid gap-2 border-b border-border/60 py-3 lg:grid-cols-[180px_120px_1fr_1fr] lg:items-end"
                    >
                      <div>
                        <div className="text-xs font-medium">{feature.label}</div>
                        <div className="font-mono text-2xs text-muted-foreground">
                          {feature.key}
                        </div>
                      </div>
                      <Field label="Enabled">
                        <Select
                          value={
                            override?.enabled == null
                              ? "inherit"
                              : override.enabled
                                ? "enabled"
                                : "disabled"
                          }
                          onValueChange={(value) =>
                            setDraft(
                              updateLlmOverride(draft, feature.key, {
                                enabled: value === "inherit" ? null : value === "enabled",
                              }),
                            )
                          }
                        >
                          <SelectTrigger
                            className="h-9 bg-background"
                            aria-label={`${feature.label} enabled`}
                          >
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="inherit">Inherit</SelectItem>
                            <SelectItem value="enabled">Enabled</SelectItem>
                            <SelectItem value="disabled">Disabled</SelectItem>
                          </SelectContent>
                        </Select>
                      </Field>
                      <Field label="Model">
                        <Input
                          aria-label={`${feature.label} model`}
                          value={override?.model ?? ""}
                          onChange={(event) =>
                            setDraft(
                              updateLlmOverride(draft, feature.key, {
                                model: emptyToNull(event.target.value),
                              }),
                            )
                          }
                          placeholder="inherit"
                        />
                      </Field>
                      <Field label="Base URL">
                        <Input
                          aria-label={`${feature.label} base URL`}
                          value={override?.base_url ?? ""}
                          onChange={(event) =>
                            setDraft(
                              updateLlmOverride(draft, feature.key, {
                                base_url: emptyToNull(event.target.value),
                              }),
                            )
                          }
                          placeholder="inherit"
                        />
                      </Field>
                    </div>
                  );
                })}
              </div>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
              <div className="text-2xs text-muted-foreground">
                {currentStatus?.api_key_env
                  ? `API key env ${currentStatus.api_key_env}: ${currentStatus.api_key_present ? "present" : "missing"}`
                  : "No API key env configured."}
              </div>
              <Button
                onClick={() => draft && save.mutate(draft)}
                disabled={!isValid || save.isPending || configUnsupported}
              >
                {configUnsupported ? "Daemon update required" : "Save LLM config"}
              </Button>
            </div>
          </section>
        ) : (
          <PageSkeleton rows={4} label="Loading LLM config" />
        )}
      </div>
    </div>
  );
}

function llmConfigFromStatus(status: LlmStatus): LlmConfig {
  return {
    enabled: status.enabled,
    base_url: status.base_url ?? defaultLlmConfig.base_url,
    model:
      status.configured_model.trim() ||
      (status.model === "noop" ? "" : status.model.trim()) ||
      defaultLlmConfig.model,
    api_key_env: status.api_key_env ?? "",
    context_window:
      status.context_window > 0 ? status.context_window : defaultLlmConfig.context_window,
    request_timeout_secs:
      status.request_timeout_secs > 0
        ? status.request_timeout_secs
        : defaultLlmConfig.request_timeout_secs,
    allow_cloud_relationship_data: defaultLlmConfig.allow_cloud_relationship_data,
    overrides: {},
  };
}

function normalizeLlmConfig(config: LlmConfig): LlmConfig {
  return { ...config, overrides: config.overrides ?? {} };
}

function updateLlmOverride(
  config: LlmConfig,
  key: LlmOverrideKey,
  patch: LlmOverrideConfig,
): LlmConfig {
  const current = config.overrides?.[key] ?? {};
  const next = pruneOverride({ ...current, ...patch });
  return {
    ...config,
    overrides: {
      ...config.overrides,
      [key]: next,
    },
  };
}

function pruneOverride(override: LlmOverrideConfig): LlmOverrideConfig | null {
  const next: LlmOverrideConfig = {
    enabled: override.enabled ?? null,
    base_url: emptyToNull(override.base_url),
    model: emptyToNull(override.model),
    api_key_env: emptyToNull(override.api_key_env),
    context_window: override.context_window ?? null,
    request_timeout_secs: override.request_timeout_secs ?? null,
  };
  const hasValue = Object.values(next).some((value) => value !== null && value !== undefined);
  return hasValue ? next : null;
}

function emptyToNull(value?: string | null): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : null;
}

function isNotFoundError(error: unknown): boolean {
  // BridgeRequestError carries the HTTP status; older errors put it in the text.
  if (error instanceof Error && "status" in error && typeof error.status === "number")
    return error.status === 404;
  return error instanceof Error && /^404\b/.test(error.message);
}
