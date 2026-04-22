import { Tabs } from "@base-ui/react";
import { MailWarning } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type {
  BridgeState,
  DiagnosticsWorkspaceSection,
  DiagnosticsWorkspaceState,
  SavedDraftSummary,
  SidebarItem,
  SnoozedMessageSummary,
  SubscriptionSummary,
} from "../../shared/types";
import { cn } from "../lib/cn";
import { useDesktopSettings } from "../lib/theme";
import {
  createEffectiveKeymap,
  formatKeymapBindings,
  parseKeymapBindings,
  serializeKeymapBindings,
} from "../lib/keymap";
import { desktopThemes } from "../lib/themes";
import { HeaderActionButton, StatCard } from "./shared";

export function DiagnosticsWorkspace(props: {
  bridge: Extract<BridgeState, { kind: "ready" }>;
  diagnostics: DiagnosticsWorkspaceState | null;
  activeSection: DiagnosticsWorkspaceSection;
  onSectionChange: (section: DiagnosticsWorkspaceSection) => void;
  labels: SidebarItem[];
  savedSearches: SidebarItem[];
  onGenerateBugReport: () => void;
  onResumeDraft: (draft: SavedDraftSummary) => void;
  onOpenSubscription: (subscription: SubscriptionSummary) => void;
  onOpenSnoozed: (message: SnoozedMessageSummary) => void;
  onSemanticReindex: () => void;
  onCreateLabel: (name: string) => void;
  onRenameLabel: (oldName: string, newName: string) => void;
  onDeleteLabel: (name: string) => void;
  onDeleteSavedSearch: (name: string) => void;
}) {
  const [newLabelName, setNewLabelName] = useState("");
  const [editingLabel, setEditingLabel] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState("");
  const [keymapText, setKeymapText] = useState("");
  const [keymapError, setKeymapError] = useState<string | null>(null);
  const [keymapStatus, setKeymapStatus] = useState<string | null>(null);
  const { theme, setTheme, settings, updateDesktopSettings } =
    useDesktopSettings();

  const semanticStatus = props.diagnostics?.semanticStatus;
  const workspaceTabClass = cn(
    "flex items-center gap-1.5 border border-outline bg-canvas-elevated px-2 py-1 text-[length:var(--text-xs)] uppercase text-foreground-subtle transition-colors",
    "data-[selected]:border-accent/30 data-[selected]:bg-accent/12 data-[selected]:text-accent",
    "hover:text-foreground",
  );
  const workspaceCounts = {
    drafts: props.diagnostics?.drafts.length ?? 0,
    subscriptions: props.diagnostics?.subscriptions.length ?? 0,
    snoozed: props.diagnostics?.snoozed.length ?? 0,
    labels: props.labels.length,
    savedSearches: props.savedSearches.length,
  };
  const effectiveKeymap = useMemo(
    () => createEffectiveKeymap(settings.keymapOverrides),
    [settings.keymapOverrides],
  );

  useEffect(() => {
    setKeymapText(
      formatKeymapBindings(serializeKeymapBindings(effectiveKeymap)),
    );
    setKeymapError(null);
  }, [effectiveKeymap]);

  const saveKeymap = async () => {
    try {
      const parsed = parseKeymapBindings(keymapText);
      await updateDesktopSettings({ keymapOverrides: parsed });
      setKeymapError(null);
      setKeymapStatus("Keymap saved");
    } catch (error) {
      setKeymapStatus(null);
      setKeymapError(
        error instanceof Error ? error.message : "Invalid keymap JSON",
      );
    }
  };

  return (
    <div className="subtle-scrollbar h-full overflow-y-auto bg-panel-muted px-4 py-4">
      <section className="surface mx-auto flex w-full max-w-6xl flex-col gap-4 px-4 py-4">
        <div className="flex items-center gap-3">
          <div className="border border-outline bg-panel-elevated p-2">
            <MailWarning className="size-5 text-warning" />
          </div>
          <div>
            <p className="mono-meta">Diagnostics</p>
            <h1 className="mt-1 text-2xl font-semibold tracking-tight text-foreground">
              Diagnostics
            </h1>
          </div>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <HeaderActionButton
            label="Generate bug report"
            onClick={props.onGenerateBugReport}
          />
        </div>
        <Tabs.Root
          value={props.activeSection}
          onValueChange={(value) =>
            props.onSectionChange(
              (value ?? "overview") as DiagnosticsWorkspaceSection,
            )
          }
          className="space-y-4"
        >
          <Tabs.List className="flex flex-wrap gap-1">
            <Tabs.Tab
              value="overview"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Overview
            </Tabs.Tab>
            <Tabs.Tab
              value="drafts"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Drafts
              <TabCount value={workspaceCounts.drafts} />
            </Tabs.Tab>
            <Tabs.Tab
              value="subscriptions"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Subscriptions
              <TabCount value={workspaceCounts.subscriptions} />
            </Tabs.Tab>
            <Tabs.Tab
              value="snoozed"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Snoozed
              <TabCount value={workspaceCounts.snoozed} />
            </Tabs.Tab>
            <Tabs.Tab
              value="semantic"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Semantic
            </Tabs.Tab>
            <Tabs.Tab
              value="labels"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Labels
              <TabCount value={workspaceCounts.labels} />
            </Tabs.Tab>
            <Tabs.Tab
              value="saved-searches"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Saved Searches
              <TabCount value={workspaceCounts.savedSearches} />
            </Tabs.Tab>
            <Tabs.Tab
              value="settings"
              className={workspaceTabClass}
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              Settings
            </Tabs.Tab>
          </Tabs.List>

          <Tabs.Panel value="overview" className="space-y-4">
            <div className="grid gap-4 md:grid-cols-4">
              <StatCard
                label="Daemon version"
                value={props.bridge.daemonVersion ?? "unknown"}
              />
              <StatCard
                label="Protocol"
                value={String(props.bridge.protocolVersion)}
              />
              <StatCard
                label="Health"
                value={props.diagnostics?.report.health_class ?? "loading"}
              />
              <StatCard
                label="Semantic queue"
                value={String(semanticStatus?.runtime.queue_depth ?? 0)}
              />
            </div>
            <div className="grid gap-4 xl:grid-cols-2">
              <Panel
                title="Mail workflow surfaces"
                subtitle="Dedicated spaces for recoverable work"
              >
                <ActionRow
                  title="Saved drafts"
                  detail="Recover and reopen drafts that were saved locally."
                  meta={`${workspaceCounts.drafts} recoverable`}
                  onPrimary={() => {
                    const draft = props.diagnostics?.drafts[0];
                    if (draft) {
                      props.onResumeDraft(draft);
                    }
                  }}
                  primaryLabel={
                    workspaceCounts.drafts > 0 ? "Resume latest" : "Ready"
                  }
                  primaryDisabled={workspaceCounts.drafts === 0}
                />
                <ActionRow
                  title="Subscriptions"
                  detail="Open newsletter senders as a focused mailbox lens."
                  meta={`${workspaceCounts.subscriptions} tracked`}
                  onPrimary={() => {
                    const subscription = props.diagnostics?.subscriptions[0];
                    if (subscription) {
                      props.onOpenSubscription(subscription);
                    }
                  }}
                  primaryLabel={
                    workspaceCounts.subscriptions > 0 ? "Open latest" : "Ready"
                  }
                  primaryDisabled={workspaceCounts.subscriptions === 0}
                />
                <ActionRow
                  title="Snoozed"
                  detail="Jump back into mail waiting on a wake-up time."
                  meta={`${workspaceCounts.snoozed} queued`}
                  onPrimary={() => {
                    const message = props.diagnostics?.snoozed[0];
                    if (message) {
                      props.onOpenSnoozed(message);
                    }
                  }}
                  primaryLabel={
                    workspaceCounts.snoozed > 0 ? "Open latest" : "Ready"
                  }
                  primaryDisabled={workspaceCounts.snoozed === 0}
                />
              </Panel>
              <Panel title="Recommended next steps">
                {(props.diagnostics?.report.recommended_next_steps ?? [])
                  .length === 0 ? (
                  <p className="text-sm text-foreground-muted">
                    No follow-up actions reported.
                  </p>
                ) : (
                  props.diagnostics?.report.recommended_next_steps.map(
                    (item) => (
                      <p
                        key={item}
                        className="text-sm leading-6 text-foreground-muted"
                      >
                        {item}
                      </p>
                    ),
                  )
                )}
              </Panel>
            </div>
            <Panel title="Recent errors">
              {(props.diagnostics?.report.recent_error_logs ?? []).length ===
              0 ? (
                <p className="text-sm text-foreground-muted">
                  No recent error logs.
                </p>
              ) : (
                props.diagnostics?.report.recent_error_logs.map((item) => (
                  <p
                    key={item}
                    className="font-mono text-xs leading-6 text-foreground-muted"
                  >
                    {item}
                  </p>
                ))
              )}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="drafts">
            <Panel
              title="Saved drafts"
              subtitle={`${workspaceCounts.drafts} drafts recoverable`}
            >
              {(props.diagnostics?.drafts ?? []).map((draft) => (
                <ActionRow
                  key={draft.id}
                  title={draft.subject || "(no subject)"}
                  detail={draft.recipients || "No recipients yet"}
                  meta={
                    draft.attachment_count > 0
                      ? `${draft.attachment_count} attachments`
                      : "Draft"
                  }
                  onPrimary={() => props.onResumeDraft(draft)}
                  primaryLabel="Resume"
                />
              ))}
              {(props.diagnostics?.drafts ?? []).length === 0 ? (
                <p className="text-sm text-foreground-muted">
                  No saved drafts on disk.
                </p>
              ) : null}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="subscriptions">
            <Panel
              title="Subscriptions"
              subtitle={`${workspaceCounts.subscriptions} senders tracked`}
            >
              {(props.diagnostics?.subscriptions ?? []).map((subscription) => (
                <ActionRow
                  key={subscription.sender_email}
                  title={subscription.sender_name || subscription.sender_email}
                  detail={subscription.latest_subject}
                  meta={`${subscription.message_count} messages`}
                  onPrimary={() => props.onOpenSubscription(subscription)}
                  primaryLabel="Open"
                />
              ))}
              {(props.diagnostics?.subscriptions ?? []).length === 0 ? (
                <p className="text-sm text-foreground-muted">
                  No subscriptions detected.
                </p>
              ) : null}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="snoozed">
            <Panel
              title="Snoozed"
              subtitle={`${workspaceCounts.snoozed} messages waiting`}
            >
              {(props.diagnostics?.snoozed ?? []).map((message) => (
                <ActionRow
                  key={message.message_id}
                  title={message.subject}
                  detail={`${message.sender} · wakes ${formatShortDate(message.wake_at)}`}
                  meta={message.unread ? "Unread" : "Read"}
                  onPrimary={() => props.onOpenSnoozed(message)}
                  primaryLabel="Open"
                />
              ))}
              {(props.diagnostics?.snoozed ?? []).length === 0 ? (
                <p className="text-sm text-foreground-muted">
                  No snoozed messages queued.
                </p>
              ) : null}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="semantic" className="space-y-4">
            <div className="flex justify-end">
              <HeaderActionButton
                label="Reindex semantic"
                onClick={props.onSemanticReindex}
              />
            </div>
            <div className="grid gap-4 md:grid-cols-4">
              <StatCard
                label="Enabled"
                value={semanticStatus?.enabled ? "yes" : "no"}
              />
              <StatCard
                label="Profile"
                value={semanticStatus?.active_profile ?? "disabled"}
              />
              <StatCard
                label="Queue depth"
                value={String(semanticStatus?.runtime.queue_depth ?? 0)}
              />
              <StatCard
                label="In flight"
                value={String(semanticStatus?.runtime.in_flight ?? 0)}
              />
            </div>
            <div className="grid gap-4 xl:grid-cols-2">
              <Panel title="Profiles">
                {(semanticStatus?.profiles ?? []).length === 0 ? (
                  <p className="text-sm text-foreground-muted">
                    No semantic profiles configured.
                  </p>
                ) : (
                  semanticStatus?.profiles.map((profile) => (
                    <div
                      key={profile.id}
                      className="flex items-center justify-between gap-3 border-t border-outline/60 py-2 first:border-t-0 first:pt-0"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm text-foreground">
                          {profile.profile}
                        </p>
                        <p className="text-xs text-foreground-subtle">
                          {profile.dimensions} dims
                        </p>
                      </div>
                      <span className="text-xs text-foreground-subtle">
                        {profile.enabled ? "Enabled" : "Disabled"}
                      </span>
                    </div>
                  ))
                )}
              </Panel>
              <Panel title="Runtime">
                <RuntimeMetric
                  label="Last queue wait"
                  value={formatDuration(
                    semanticStatus?.runtime.last_queue_wait_ms,
                  )}
                />
                <RuntimeMetric
                  label="Last extract"
                  value={formatDuration(
                    semanticStatus?.runtime.last_extract_ms,
                  )}
                />
                <RuntimeMetric
                  label="Last embedding prep"
                  value={formatDuration(
                    semanticStatus?.runtime.last_embedding_prep_ms,
                  )}
                />
                <RuntimeMetric
                  label="Last ingest"
                  value={formatDuration(semanticStatus?.runtime.last_ingest_ms)}
                />
              </Panel>
            </div>
          </Tabs.Panel>

          <Tabs.Panel value="labels">
            <Panel
              title="Labels"
              subtitle={`${workspaceCounts.labels} editable labels`}
            >
              <form
                className="mb-3 flex gap-2"
                onSubmit={(event) => {
                  event.preventDefault();
                  if (!newLabelName.trim()) {
                    return;
                  }
                  props.onCreateLabel(newLabelName.trim());
                  setNewLabelName("");
                }}
              >
                <input
                  className="min-w-0 flex-1 border border-outline bg-canvas-elevated px-2 py-1.5 text-sm text-foreground outline-none"
                  style={{ borderRadius: "var(--radius-sm)" }}
                  value={newLabelName}
                  onChange={(event) => setNewLabelName(event.target.value)}
                  placeholder="Create label"
                />
                <button
                  type="submit"
                  className="border border-outline px-2 py-1.5 text-xs text-foreground-muted hover:bg-panel-elevated hover:text-foreground"
                  style={{ borderRadius: "var(--radius-sm)" }}
                >
                  Create
                </button>
              </form>
              {props.labels.map((label) => (
                <div
                  key={label.id}
                  className="flex items-center justify-between gap-3 border-t border-outline/60 py-2 first:border-t-0 first:pt-0"
                >
                  {editingLabel === label.id ? (
                    <form
                      className="flex min-w-0 flex-1 gap-2"
                      onSubmit={(event) => {
                        event.preventDefault();
                        if (!editingValue.trim()) {
                          return;
                        }
                        props.onRenameLabel(label.label, editingValue.trim());
                        setEditingLabel(null);
                        setEditingValue("");
                      }}
                    >
                      <input
                        className="min-w-0 flex-1 border border-outline bg-canvas-elevated px-2 py-1 text-sm text-foreground outline-none"
                        style={{ borderRadius: "var(--radius-sm)" }}
                        value={editingValue}
                        onChange={(event) =>
                          setEditingValue(event.target.value)
                        }
                      />
                      <button
                        type="submit"
                        className="border border-outline px-2 py-1 text-xs text-foreground-muted hover:bg-panel-elevated hover:text-foreground"
                        style={{ borderRadius: "var(--radius-sm)" }}
                      >
                        Save
                      </button>
                    </form>
                  ) : (
                    <>
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm text-foreground">
                          {label.label}
                        </p>
                        <p className="text-xs text-foreground-subtle">
                          {label.total} total
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          className="text-xs text-accent"
                          onClick={() => {
                            setEditingLabel(label.id);
                            setEditingValue(label.label);
                          }}
                        >
                          Rename
                        </button>
                        <button
                          type="button"
                          className="text-xs text-danger"
                          onClick={() => props.onDeleteLabel(label.label)}
                        >
                          Delete
                        </button>
                      </div>
                    </>
                  )}
                </div>
              ))}
              {props.labels.length === 0 ? (
                <p className="text-sm text-foreground-muted">
                  No editable labels.
                </p>
              ) : null}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="saved-searches">
            <Panel
              title="Saved searches"
              subtitle={`${workspaceCounts.savedSearches} configured`}
            >
              {props.savedSearches.map((search) => (
                <div
                  key={search.id}
                  className={cn(
                    "flex items-center justify-between gap-3 border-t border-outline/60 py-2",
                    "first:border-t-0 first:pt-0",
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm text-foreground">
                      {search.label}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="text-xs text-danger"
                    onClick={() => props.onDeleteSavedSearch(search.label)}
                  >
                    Delete
                  </button>
                </div>
              ))}
              {props.savedSearches.length === 0 ? (
                <p className="text-sm text-foreground-muted">
                  No saved searches configured.
                </p>
              ) : null}
            </Panel>
          </Tabs.Panel>

          <Tabs.Panel value="settings">
            <div className="grid gap-4 xl:grid-cols-2">
              <Panel
                title="Appearance"
                subtitle="Desktop presentation stays local to this machine"
              >
                <label className="flex flex-col gap-1.5 text-sm text-foreground">
                  <span className="mono-meta">Theme</span>
                  <select
                    aria-label="Theme"
                    className="border border-outline bg-canvas-elevated px-2 py-1.5 text-sm text-foreground outline-none"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    value={theme}
                    onChange={(event) =>
                      void setTheme(event.target.value as typeof theme)
                    }
                  >
                    {desktopThemes.map((option) => (
                      <option key={option.id} value={option.id}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
              </Panel>

              <Panel
                title="Keymaps"
                subtitle="JSONC overrides by context then shortcut"
              >
                <label className="flex flex-col gap-1.5 text-sm text-foreground">
                  <span className="mono-meta">Keymap JSON</span>
                  <textarea
                    aria-label="Keymap JSON"
                    className="min-h-64 border border-outline bg-canvas-elevated px-3 py-2 font-mono text-xs text-foreground outline-none"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    value={keymapText}
                    onChange={(event) => {
                      setKeymapText(event.target.value);
                      setKeymapError(null);
                      setKeymapStatus(null);
                    }}
                    spellCheck={false}
                  />
                </label>
                <p className="text-xs leading-5 text-foreground-subtle">
                  Contexts: <code>mailList</code>, <code>threadView</code>,{" "}
                  <code>messageView</code>, <code>rules</code>,{" "}
                  <code>accounts</code>, <code>diagnostics</code>.
                </p>
                {keymapError ? (
                  <p className="text-xs text-danger">{keymapError}</p>
                ) : null}
                {keymapStatus ? (
                  <p className="text-xs text-success">{keymapStatus}</p>
                ) : null}
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    className="border border-outline px-2 py-1.5 text-xs text-foreground-muted hover:bg-panel hover:text-foreground"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    onClick={() => void saveKeymap()}
                  >
                    Save keymap
                  </button>
                  <button
                    type="button"
                    className="border border-outline px-2 py-1.5 text-xs text-foreground-subtle hover:bg-panel hover:text-foreground"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    onClick={() =>
                      void updateDesktopSettings({ keymapOverrides: {} })
                    }
                  >
                    Reset to defaults
                  </button>
                </div>
              </Panel>
            </div>
          </Tabs.Panel>
        </Tabs.Root>
      </section>
    </div>
  );
}

function TabCount(props: { value: number }) {
  return (
    <span className="font-mono text-[length:var(--text-2xs)] tabular-nums text-foreground-subtle">
      {props.value}
    </span>
  );
}

function Panel(props: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <section className="border border-outline bg-panel-elevated px-3 py-3">
      <div className="mb-3 flex items-end justify-between gap-3">
        <div>
          <p className="mono-meta">{props.title}</p>
          {props.subtitle ? (
            <p className="mt-1 text-xs text-foreground-subtle">
              {props.subtitle}
            </p>
          ) : null}
        </div>
      </div>
      <div className="space-y-2">{props.children}</div>
    </section>
  );
}

function ActionRow(props: {
  title: string;
  detail: string;
  meta: string;
  primaryLabel: string;
  onPrimary: () => void;
  primaryDisabled?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-t border-outline/60 py-2 first:border-t-0 first:pt-0">
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-foreground">{props.title}</p>
        <p className="truncate text-xs text-foreground-muted">{props.detail}</p>
      </div>
      <div className="flex items-center gap-2">
        <span className="text-xs text-foreground-subtle">{props.meta}</span>
        <button
          type="button"
          disabled={props.primaryDisabled}
          className={cn(
            "border border-outline px-2 py-1 text-xs text-foreground-muted",
            props.primaryDisabled
              ? "opacity-60"
              : "hover:bg-panel hover:text-foreground",
          )}
          style={{ borderRadius: "var(--radius-sm)" }}
          onClick={props.onPrimary}
        >
          {props.primaryLabel}
        </button>
      </div>
    </div>
  );
}

function formatShortDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function RuntimeMetric(props: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 border-t border-outline/60 py-2 first:border-t-0 first:pt-0">
      <span className="text-sm text-foreground">{props.label}</span>
      <span className="font-mono text-xs text-foreground-subtle">
        {props.value}
      </span>
    </div>
  );
}

function formatDuration(value: number | null | undefined) {
  if (value == null) {
    return "n/a";
  }
  return `${value} ms`;
}
