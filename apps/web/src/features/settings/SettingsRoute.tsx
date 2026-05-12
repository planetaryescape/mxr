import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import {
  Bell,
  BookOpen,
  Bot,
  Code2,
  Info,
  Keyboard,
  Layers,
  Palette,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import { Textarea } from "@/components/ui/textarea";
import { apiFetch } from "@/api/client";
import { TokenSection } from "@/features/settings/TokenSection";
import {
  useUiPrefs,
  type ComposeEditor,
  type Density,
  type EmailHtmlTheme,
  type ReaderLayout,
  type Theme,
} from "@/state/uiPrefsStore";

const sections = [
  ["theme", "Theme", Palette],
  ["density", "Density", Layers],
  ["reader", "Reader", BookOpen],
  ["keybindings", "Keybindings", Keyboard],
  ["notifications", "Notifications", Bell],
  ["compose", "Compose", Pencil],
  ["voice", "Voice", Bot],
  ["llm", "LLM", Bot],
  ["snippets", "Snippets", Code2],
  ["token", "Token", Code2],
  ["about", "About", Info],
] as const;

interface Snippet {
  name: string;
  body: string;
  vars?: string[];
  updated_at?: string;
}

interface LlmConfig {
  enabled: boolean;
  base_url: string;
  model: string;
  api_key_env: string;
  context_window: number;
  request_timeout_secs: number;
}

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

interface UserVoiceProfile {
  account_id: string;
  formality_score: number;
  avg_sentence_len: number;
  msg_count_used: number;
  register_modes?: Array<{
    register: string;
    formality_score: number;
    avg_sentence_len: number;
    exemplar_message_ids?: string[];
  }>;
  computed_at: string;
}

const defaultLlmConfig: LlmConfig = {
  enabled: false,
  base_url: "http://localhost:11434/v1",
  model: "qwen2.5:3b-instruct",
  api_key_env: "",
  context_window: 8192,
  request_timeout_secs: 120,
};

export function SettingsRoute() {
  const { section } = useParams({ from: "/settings/$section" });
  return (
    <div className="grid min-w-0 flex-1 grid-cols-[220px_1fr] bg-background">
      <aside className="border-r border-border bg-surface p-3">
        <div className="mb-3 px-2 font-mono text-2xs uppercase tracking-wide text-muted-foreground">
          Settings
        </div>
        <nav className="space-y-1">
          {sections.map(([id, label, Icon]) => (
            <Link
              key={id}
              to="/settings/$section"
              params={{ section: id }}
              className={
                id === section
                  ? "flex items-center gap-2 rounded-md bg-primary-muted px-2 py-1.5 text-xs text-foreground"
                  : "flex items-center gap-2 rounded-md px-2 py-1.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
              }
            >
              <Icon className="size-3.5" />
              {label}
            </Link>
          ))}
        </nav>
      </aside>
      <main className="min-h-0 overflow-auto">
        {section === "token" ? <TokenSection /> : <SettingsSection section={section} />}
      </main>
    </div>
  );
}

function SettingsSection({ section }: { section: string }) {
  const theme = useUiPrefs((state) => state.theme);
  const density = useUiPrefs((state) => state.density);
  const composeEditor = useUiPrefs((state) => state.composeEditor);
  const emailHtmlTheme = useUiPrefs((state) => state.emailHtmlTheme);
  const readerLayout = useUiPrefs((state) => state.readerLayout);
  const notificationsEnabled = useUiPrefs((state) => state.notificationsEnabled);
  const notifyAllNewMail = useUiPrefs((state) => state.notifyAllNewMail);
  const vipAllowlist = useUiPrefs((state) => state.vipAllowlist);
  const setTheme = useUiPrefs((state) => state.setTheme);
  const setDensity = useUiPrefs((state) => state.setDensity);
  const setComposeEditor = useUiPrefs((state) => state.setComposeEditor);
  const setEmailHtmlTheme = useUiPrefs((state) => state.setEmailHtmlTheme);
  const setReaderLayout = useUiPrefs((state) => state.setReaderLayout);
  const setNotificationsEnabled = useUiPrefs((state) => state.setNotificationsEnabled);
  const setNotifyAllNewMail = useUiPrefs((state) => state.setNotifyAllNewMail);
  const addVip = useUiPrefs((state) => state.addVip);
  const removeVip = useUiPrefs((state) => state.removeVip);
  const [vip, setVip] = useState("");

  if (section === "theme")
    return (
      <Shell title="Theme">
        <SelectRow
          label="Theme"
          value={theme}
          values={["midnight", "eclipse", "paper", "light", "system"]}
          onChange={(value) => setTheme(value as Theme)}
        />
        <SelectRow
          label="HTML email"
          value={emailHtmlTheme}
          values={["dark", "original"]}
          onChange={(value) => setEmailHtmlTheme(value as EmailHtmlTheme)}
        />
        <p className="mt-3 text-xs text-muted-foreground">
          Dark makes HTML mail use a dark canvas by default while preserving images and links.
        </p>
      </Shell>
    );
  if (section === "density")
    return (
      <Shell title="Density">
        <SelectRow
          label="Density"
          value={density}
          values={["compact", "regular", "comfortable"]}
          onChange={(value) => setDensity(value as Density)}
        />
      </Shell>
    );
  if (section === "reader")
    return (
      <Shell title="Reader">
        <SelectRow
          label="Default reader layout"
          value={readerLayout}
          values={["split", "full"]}
          onChange={(value) => setReaderLayout(value as ReaderLayout)}
        />
      </Shell>
    );
  if (section === "compose")
    return (
      <Shell title="Compose">
        <SelectRow
          label="Editor"
          value={composeEditor}
          values={["codemirror-vim", "tiptap"]}
          onChange={(value) => setComposeEditor(value as ComposeEditor)}
        />
        <p className="mt-3 text-xs text-muted-foreground">
          Signature sync is pending a daemon prefs endpoint. Snippets work today.
        </p>
      </Shell>
    );
  if (section === "notifications")
    return (
      <Shell title="Notifications">
        <div className="space-y-4">
          <Toggle
            label="Browser notifications"
            checked={notificationsEnabled}
            onChange={async (checked) => {
              if (checked && Notification.permission === "default")
                await Notification.requestPermission();
              setNotificationsEnabled(checked);
            }}
          />
          <Toggle
            label="Notify on all new mail"
            checked={notifyAllNewMail}
            onChange={setNotifyAllNewMail}
          />
          <div className="space-y-2">
            <Label>VIP allowlist</Label>
            <div className="flex gap-2">
              <Input
                value={vip}
                onChange={(event) => setVip(event.target.value)}
                placeholder="alice@example.com or @acme.com"
              />
              <Button
                onClick={() => {
                  addVip(vip.trim());
                  setVip("");
                }}
                disabled={!vip.trim()}
              >
                Add
              </Button>
            </div>
            <div className="flex flex-wrap gap-2">
              {vipAllowlist.map((item) => (
                <Badge key={item} variant="outline" className="py-1">
                  {item}
                  <button onClick={() => removeVip(item)}>
                    <Trash2 className="size-3" />
                  </button>
                </Badge>
              ))}
            </div>
          </div>
        </div>
      </Shell>
    );
  if (section === "keybindings")
    return (
      <Shell title="Keybindings">
        <div className="grid gap-2">
          {[
            ["Command palette", "⌘K"],
            ["Focus search", "/"],
            ["Help", "?"],
            ["Compose", "c"],
            ["Inbox", "g i"],
            ["Rules", "g r"],
          ].map(([label, key]) => (
            <div
              key={label}
              className="flex items-center justify-between rounded-md border border-border px-3 py-2 text-xs"
            >
              <span>{label}</span>
              <KeyChip>{key}</KeyChip>
            </div>
          ))}
        </div>
      </Shell>
    );
  if (section === "voice") return <VoiceSettingsSection />;
  if (section === "llm") return <LlmSettingsSection />;
  if (section === "snippets") return <SnippetsSection />;
  if (section === "about")
    return (
      <Shell title="About">
        <pre className="rounded-lg bg-muted p-3 text-2xs">
          {JSON.stringify(
            {
              app: "mxr web",
              version: import.meta.env.PACKAGE_VERSION ?? "dev",
              bridge: "local daemon HTTP/WebSocket",
            },
            null,
            2,
          )}
        </pre>
      </Shell>
    );
  return (
    <Shell title="Settings">
      <p className="text-xs text-muted-foreground">Unknown section.</p>
    </Shell>
  );
}

function VoiceSettingsSection() {
  const [accountId, setAccountId] = useState("");
  const voice = useQuery({
    queryKey: ["user-voice", accountId],
    queryFn: () =>
      apiFetch<{ profile: UserVoiceProfile }>(
        `/api/v1/platform/voice?account_id=${encodeURIComponent(accountId)}`,
      ),
    enabled: accountId.trim().length > 0,
  });
  const rebuild = useMutation({
    mutationFn: () =>
      apiFetch<{ profile: UserVoiceProfile }>(
        `/api/v1/platform/voice/rebuild?account_id=${encodeURIComponent(accountId)}`,
        { method: "POST" },
      ),
    onSuccess: () => {
      toast.success("Voice profile rebuilt");
      void voice.refetch();
    },
    onError: (error) => toast.error("Voice rebuild failed", { description: error.message }),
  });
  const profile = voice.data?.profile;
  return (
    <Shell title="Voice">
      <div className="space-y-4">
        <Card className="space-y-3 p-4">
          <div>
            <h2 className="text-sm font-semibold">Inspectable user voice</h2>
            <p className="mt-1 text-xs text-muted-foreground">
              Paste an account id to inspect or rebuild the local outbound voice profile.
            </p>
          </div>
          <div className="flex gap-2">
            <Input
              value={accountId}
              onChange={(event) => setAccountId(event.target.value)}
              placeholder="account UUID"
              aria-label="Account id"
            />
            <Button
              variant="outline"
              disabled={!accountId.trim() || rebuild.isPending}
              onClick={() => rebuild.mutate()}
            >
              Rebuild
            </Button>
          </div>
        </Card>
        {voice.isError ? (
          <Alert variant="warning" className="text-xs">
            Could not load voice profile. It may need at least 20 outbound messages.
          </Alert>
        ) : null}
        {profile ? (
          <div className="grid gap-3 md:grid-cols-3">
            <VoiceCard
              title="Overall"
              formality={profile.formality_score}
              sentenceLen={profile.avg_sentence_len}
              samples={profile.msg_count_used}
            />
            {(profile.register_modes ?? []).map((mode) => (
              <VoiceCard
                key={mode.register}
                title={mode.register}
                formality={mode.formality_score}
                sentenceLen={mode.avg_sentence_len}
                samples={mode.exemplar_message_ids?.length ?? 0}
              />
            ))}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            {accountId ? "Loading voice profile..." : "No account selected."}
          </p>
        )}
      </div>
    </Shell>
  );
}

function VoiceCard({
  title,
  formality,
  sentenceLen,
  samples,
}: {
  title: string;
  formality: number;
  sentenceLen: number;
  samples: number;
}) {
  return (
    <Card className="space-y-2 p-4">
      <h3 className="text-sm font-semibold capitalize">{title}</h3>
      <ProfileLikeRow label="Formality" value={formality.toFixed(2)} />
      <ProfileLikeRow label="Sentence len" value={sentenceLen.toFixed(1)} />
      <ProfileLikeRow label="Samples" value={String(samples)} />
    </Card>
  );
}

function ProfileLikeRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono text-foreground">{value}</span>
    </div>
  );
}

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
      setDraft(config.data.config);
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
    <Shell title="LLM">
      <div className="space-y-4">
        <Card className="p-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="text-sm font-semibold">Thread summaries and draft assist</h2>
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
        </Card>

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
          <Card className="space-y-4 p-4">
            <div className="flex items-center justify-between rounded-lg border border-border bg-background px-3 py-2">
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
          </Card>
        ) : (
          <p className="text-xs text-muted-foreground">Loading LLM config...</p>
        )}
      </div>
    </Shell>
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
  };
}

function isNotFoundError(error: unknown): boolean {
  return error instanceof Error && /^404\b/.test(error.message);
}

function SnippetsSection() {
  const qc = useQueryClient();
  const snippets = useQuery({
    queryKey: ["snippets"],
    queryFn: () => apiFetch<{ snippets: Snippet[] }>("/api/v1/mail/snippets"),
  });
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const save = useMutation({
    mutationFn: () =>
      apiFetch<unknown>("/api/v1/mail/snippets", {
        method: "POST",
        body: { name, body, vars: [] },
      }),
    onSuccess: () => {
      toast.success("Snippet saved");
      setName("");
      setBody("");
      void qc.invalidateQueries({ queryKey: ["snippets"] });
    },
  });
  const remove = useMutation({
    mutationFn: (snippet: string) =>
      apiFetch<unknown>(`/api/v1/mail/snippets/${encodeURIComponent(snippet)}`, {
        method: "DELETE",
      }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["snippets"] }),
  });
  return (
    <Shell title="Snippets">
      <div className="grid gap-4 lg:grid-cols-[360px_1fr]">
        <Card className="space-y-3 p-4">
          <Field label="Name">
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="sig"
            />
          </Field>
          <Field label="Body">
            <Textarea
              value={body}
              onChange={(event) => setBody(event.target.value)}
              className="min-h-32"
              placeholder="Best,\nYou"
            />
          </Field>
          <Button
            onClick={() => save.mutate()}
            disabled={!name.trim() || !body.trim() || save.isPending}
          >
            <Plus className="size-3" />
            Save snippet
          </Button>
        </Card>
        <Card className="p-4">
          <div className="divide-y divide-border">
            {(snippets.data?.snippets ?? []).map((snippet) => (
              <div key={snippet.name} className="flex items-start justify-between gap-3 py-3">
                <div>
                  <div className="text-xs font-medium">;{snippet.name}</div>
                  <pre className="mt-1 whitespace-pre-wrap text-2xs text-muted-foreground">
                    {snippet.body}
                  </pre>
                </div>
                <Button variant="ghost" size="icon" onClick={() => remove.mutate(snippet.name)}>
                  <Trash2 className="size-3" />
                </Button>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </Shell>
  );
}

function Shell({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="p-6">
      <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
      <div className="mt-5 max-w-3xl">{children}</div>
    </div>
  );
}

function SelectRow({
  label,
  value,
  values,
  onChange,
}: {
  label: string;
  value: string;
  values: string[];
  onChange: (value: string) => void;
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="w-64" aria-label={label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {values.map((item) => (
            <SelectItem key={item} value={item}>
              {item}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Card className="flex items-center justify-between p-4">
      <span className="text-sm font-medium">{label}</span>
      <Switch checked={checked} onCheckedChange={onChange} />
    </Card>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <Label>{label}</Label>
      {children}
    </div>
  );
}
