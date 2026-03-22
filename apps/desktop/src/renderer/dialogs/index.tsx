import { Dialog } from "@base-ui/react";
import type { Dispatch, SetStateAction } from "react";
import type {
  AccountOperationResponse,
  RuleFormPayload,
  SidebarItem,
  SnoozePreset,
} from "../../shared/types";
import { cn } from "../lib/cn";
import { formatBytes, formatJson } from "../surfaces/formatters";
import { HeaderActionButton } from "../surfaces/shared";

function ComposeField(props: {
  label: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="grid gap-2">
      <span className="mono-meta">{props.label}</span>
      <input
        className="rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
        value={props.value}
        placeholder={props.placeholder}
        onChange={(event) => props.onChange(event.target.value)}
      />
    </label>
  );
}

export function LabelDialog(props: {
  open: boolean;
  options: string[];
  selected: string[];
  customLabel: string;
  onClose: () => void;
  onToggle: (label: string) => void;
  onCustomLabelChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(32rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Apply label
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Add one or more labels to the selected message.
          </Dialog.Description>
          <div className="mt-5 grid gap-3">
            {props.options.map((label) => (
              <label
                key={label}
                className="flex items-center gap-3 rounded-2xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground-muted"
              >
                <input
                  type="checkbox"
                  checked={props.selected.includes(label)}
                  onChange={() => props.onToggle(label)}
                />
                <span>{label}</span>
              </label>
            ))}
          </div>
          <label className="mt-5 grid gap-2">
            <span className="mono-meta">Custom labels</span>
            <input
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
              placeholder="Follow Up, Waiting"
              value={props.customLabel}
              onChange={(event) => props.onCustomLabelChange(event.target.value)}
            />
          </label>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
              onClick={props.onSubmit}
            >
              Apply
            </button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function MoveDialog(props: {
  open: boolean;
  options: string[];
  value: string;
  onClose: () => void;
  onValueChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Move message
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Move the selected message into a different label or system mailbox.
          </Dialog.Description>
          <label className="mt-5 grid gap-2">
            <span className="mono-meta">Target</span>
            <select
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
              value={props.value}
              onChange={(event) => props.onValueChange(event.target.value)}
            >
              {props.options.map((label) => (
                <option key={label} value={label}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
              onClick={props.onSubmit}
            >
              Move
            </button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function SnoozeDialog(props: {
  open: boolean;
  presets: SnoozePreset[];
  value: string;
  onClose: () => void;
  onValueChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Snooze message
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Pick a wake-up time from the same local presets used by mxr.
          </Dialog.Description>
          <div className="mt-5 grid gap-3">
            {props.presets.map((preset) => (
              <label
                key={preset.id}
                className={cn(
                  "flex items-start gap-3 rounded-2xl border px-4 py-3 text-sm",
                  props.value === preset.id
                    ? "border-accent/35 bg-accent/10 text-foreground"
                    : "border-outline bg-panel-elevated text-foreground-muted",
                )}
              >
                <input
                  type="radio"
                  checked={props.value === preset.id}
                  onChange={() => props.onValueChange(preset.id)}
                />
                <span className="flex-1">
                  <span className="block text-foreground">{preset.label}</span>
                  <span className="mt-1 block font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
                    {preset.wakeAt}
                  </span>
                </span>
              </label>
            ))}
          </div>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
              onClick={props.onSubmit}
            >
              Snooze
            </button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function UnsubscribeDialog(props: {
  open: boolean;
  sender: string;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(26rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Unsubscribe
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm leading-7 text-foreground-muted">
            Trigger the provider unsubscribe flow for {props.sender}. Desktop keeps this explicit:
            no silent one-click mutations.
          </Dialog.Description>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-xl border border-danger/30 bg-danger/10 px-4 py-2 text-sm text-danger"
              onClick={props.onSubmit}
            >
              Unsubscribe
            </button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function GoToLabelDialog(props: {
  open: boolean;
  options: SidebarItem[];
  value: string;
  onClose: () => void;
  onValueChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Go to label
          </Dialog.Title>
          <label className="mt-5 grid gap-2">
            <span className="mono-meta">Lens</span>
            <select
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
              value={props.value}
              onChange={(event) => props.onValueChange(event.target.value)}
            >
              {props.options.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
              onClick={props.onSubmit}
            >
              Open
            </button>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function AttachmentDialog(props: {
  open: boolean;
  attachments: Array<{ id: string; filename: string; size_bytes: number; message_id: string }>;
  onClose: () => void;
  onOpen: (attachmentId: string, messageId: string) => void;
  onDownload: (attachmentId: string, messageId: string) => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(40rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Attachments
          </Dialog.Title>
          <div className="mt-5 space-y-3">
            {props.attachments.map((attachment) => (
              <div
                key={attachment.id}
                className="flex items-center justify-between gap-4 rounded-2xl border border-outline bg-panel-elevated px-4 py-4"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm text-foreground">{attachment.filename}</p>
                  <p className="mt-2 font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
                    {formatBytes(attachment.size_bytes)}
                  </p>
                </div>
                <div className="flex gap-2">
                  <HeaderActionButton
                    label="Open"
                    onClick={() => props.onOpen(attachment.id, attachment.message_id)}
                  />
                  <HeaderActionButton
                    label="Download"
                    onClick={() => props.onDownload(attachment.id, attachment.message_id)}
                  />
                </div>
              </div>
            ))}
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function LinksDialog(props: {
  open: boolean;
  links: string[];
  onClose: () => void;
  onOpen: (url: string) => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(40rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Links in thread
          </Dialog.Title>
          <div className="mt-5 space-y-3">
            {props.links.map((link) => (
              <div
                key={link}
                className="flex items-center justify-between gap-4 rounded-2xl border border-outline bg-panel-elevated px-4 py-4"
              >
                <p className="min-w-0 truncate text-sm text-foreground-muted">{link}</p>
                <HeaderActionButton label="Open" onClick={() => props.onOpen(link)} />
              </div>
            ))}
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function ReportDialog(props: {
  open: boolean;
  title: string;
  content: string;
  onClose: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed inset-6 z-40 overflow-hidden rounded-3xl border border-outline bg-panel outline-none">
          <section className="flex h-full min-h-0 flex-col">
            <div className="flex items-center justify-between gap-4 border-b border-outline px-6 py-5">
              <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
                {props.title}
              </Dialog.Title>
              <div className="flex gap-2">
                <HeaderActionButton
                  label="Copy"
                  onClick={() => void navigator.clipboard?.writeText(props.content)}
                />
                <HeaderActionButton label="Close" onClick={props.onClose} />
              </div>
            </div>
            <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto px-6 py-5">
              <pre className="whitespace-pre-wrap text-sm leading-7 text-foreground-muted">
                {props.content}
              </pre>
            </div>
          </section>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function RuleFormDialog(props: {
  open: boolean;
  busyLabel: string | null;
  form: RuleFormPayload;
  onClose: () => void;
  onChange: Dispatch<SetStateAction<RuleFormPayload>>;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(44rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-3xl border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            {props.form.id ? "Edit rule" : "New rule"}
          </Dialog.Title>
          <div className="mt-5 grid gap-4">
            <ComposeField
              label="Name"
              value={props.form.name}
              onChange={(value) => props.onChange((current) => ({ ...current, name: value }))}
            />
            <label className="grid gap-2">
              <span className="mono-meta">Condition</span>
              <textarea
                className="min-h-24 rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
                value={props.form.condition}
                onChange={(event) =>
                  props.onChange((current) => ({ ...current, condition: event.target.value }))
                }
              />
            </label>
            <label className="grid gap-2">
              <span className="mono-meta">Action</span>
              <textarea
                className="min-h-28 rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
                value={props.form.action}
                onChange={(event) =>
                  props.onChange((current) => ({ ...current, action: event.target.value }))
                }
              />
            </label>
            <div className="grid gap-4 md:grid-cols-[12rem_1fr]">
              <label className="grid gap-2">
                <span className="mono-meta">Priority</span>
                <input
                  className="rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
                  type="number"
                  value={props.form.priority}
                  onChange={(event) =>
                    props.onChange((current) => ({
                      ...current,
                      priority: Number(event.target.value) || 0,
                    }))
                  }
                />
              </label>
              <label className="flex items-center gap-3 rounded-xl border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground-muted">
                <input
                  type="checkbox"
                  checked={props.form.enabled}
                  onChange={(event) =>
                    props.onChange((current) => ({ ...current, enabled: event.target.checked }))
                  }
                />
                Enabled
              </label>
            </div>
          </div>
          <div className="mt-6 flex items-center justify-between gap-3">
            <span className="mono-meta">{props.busyLabel ?? "Ready"}</span>
            <div className="flex gap-2">
              <HeaderActionButton label="Cancel" onClick={props.onClose} />
              <button
                type="button"
                className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
                onClick={props.onSubmit}
              >
                Save rule
              </button>
            </div>
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function AccountFormDialog(props: {
  open: boolean;
  busyLabel: string | null;
  draftJson: string;
  result: AccountOperationResponse["result"] | null;
  onClose: () => void;
  onChange: (value: string) => void;
  onTest: () => void;
  onSave: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed inset-6 z-40 overflow-hidden rounded-3xl border border-outline bg-panel outline-none">
          <section className="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[minmax(0,1fr)_20rem]">
            <div className="flex min-h-0 flex-col">
              <div className="border-b border-outline px-6 py-5">
                <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
                  New account
                </Dialog.Title>
                <p className="mt-3 text-sm text-foreground-muted">
                  JSON-backed for now. Thin wrapper over the daemon account config surface.
                </p>
              </div>
              <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto px-6 py-5">
                <textarea
                  className="min-h-full w-full rounded-3xl border border-outline bg-panel-muted px-5 py-5 font-mono text-sm leading-7 text-foreground outline-none"
                  value={props.draftJson}
                  onChange={(event) => props.onChange(event.target.value)}
                />
              </div>
              <div className="flex items-center justify-between gap-3 border-t border-outline px-6 py-4">
                <span className="mono-meta">{props.busyLabel ?? "Ready"}</span>
                <div className="flex gap-2">
                  <HeaderActionButton label="Cancel" onClick={props.onClose} />
                  <HeaderActionButton label="Test" onClick={props.onTest} />
                  <button
                    type="button"
                    className="rounded-xl border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
                    onClick={props.onSave}
                  >
                    Save account
                  </button>
                </div>
              </div>
            </div>
            <aside className="hidden border-l border-outline bg-panel-muted px-5 py-5 lg:block">
              <p className="mono-meta">Last operation</p>
              <pre className="mt-4 whitespace-pre-wrap text-sm leading-7 text-foreground-muted">
                {formatJson(props.result)}
              </pre>
            </aside>
          </section>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
