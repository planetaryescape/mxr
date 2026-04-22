import { Dialog } from "@base-ui/react";
import { useEffect, useRef, useState } from "react";
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
        className="rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
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
  const popupRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [activeOptionIndex, setActiveOptionIndex] = useState(0);
  const initialOptionIndex = clampIndex(
    props.options.findIndex((label) => props.selected.includes(label)),
    props.options.length,
  );

  useEffect(() => {
    if (!props.open) {
      return;
    }
    setActiveOptionIndex(initialOptionIndex);
  }, [initialOptionIndex, props.open]);

  useEffect(() => {
    if (!props.open) {
      return;
    }
    const focusTimer = window.setTimeout(() => {
      optionRefs.current[activeOptionIndex]?.focus() ?? popupRef.current?.focus();
    }, 0);
    return () => window.clearTimeout(focusTimer);
  }, [activeOptionIndex, props.open]);

  useEffect(() => {
    if (!props.open) {
      return;
    }
    const activeOption = optionRefs.current[activeOptionIndex];
    if (activeOption && typeof activeOption.scrollIntoView === "function") {
      activeOption.scrollIntoView({ block: "nearest" });
    }
  }, [activeOptionIndex, props.open]);

  const activeLabel = props.options[activeOptionIndex] ?? null;

  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup
          ref={popupRef}
          initialFocus={false}
          tabIndex={-1}
          className="fixed left-1/2 top-1/2 z-40 w-[min(32rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none"
          onKeyDown={(event) => {
            const target = event.target;
            const isTextInput = target instanceof HTMLInputElement;

            if (isTextInput) {
              if (event.key === "Enter") {
                event.preventDefault();
                props.onSubmit();
              }
              return;
            }

            if (event.key === "j" || event.key === "ArrowDown") {
              event.preventDefault();
              setActiveOptionIndex((current) =>
                clampIndex(current + 1, props.options.length),
              );
              return;
            }

            if (event.key === "k" || event.key === "ArrowUp") {
              event.preventDefault();
              setActiveOptionIndex((current) =>
                clampIndex(current - 1, props.options.length),
              );
              return;
            }

            if (event.key === " " || event.key === "Spacebar") {
              if (!activeLabel) {
                return;
              }
              event.preventDefault();
              props.onToggle(activeLabel);
              return;
            }

            if (event.key === "Enter") {
              event.preventDefault();
              props.onSubmit();
            }
          }}
        >
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Apply label
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Toggle existing labels from the list. Use Tab only when you want the
            custom-label field.
          </Dialog.Description>
          <div
            ref={listRef}
            className="subtle-scrollbar mt-5 max-h-[min(24rem,48vh)] overflow-y-auto border border-outline bg-canvas-elevated"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            {props.options.map((label, index) => (
              <button
                key={label}
                ref={(element) => {
                  optionRefs.current[index] = element;
                }}
                type="button"
                role="checkbox"
                aria-checked={props.selected.includes(label)}
                tabIndex={index === activeOptionIndex ? 0 : -1}
                data-active={index === activeOptionIndex ? "true" : "false"}
                data-selected={props.selected.includes(label) ? "true" : "false"}
                className={cn(
                  "group flex h-[var(--row-height)] min-h-[var(--row-height)] w-full items-center gap-2.5 border-l-2 px-2.5 py-2 text-left transition-colors",
                  index === activeOptionIndex && props.selected.includes(label)
                    ? "border-l-accent bg-accent/10"
                    : index === activeOptionIndex
                      ? "border-l-accent bg-panel-elevated"
                      : props.selected.includes(label)
                        ? "border-l-success/60 bg-success/6"
                        : "border-l-transparent hover:bg-panel-elevated/50",
                )}
                onFocus={() => setActiveOptionIndex(index)}
                onMouseEnter={() => setActiveOptionIndex(index)}
                onClick={() => props.onToggle(label)}
              >
                <span
                  aria-hidden="true"
                  className={cn(
                    "mt-[1px] flex size-5 shrink-0 items-center justify-center rounded-sm border transition-colors",
                    props.selected.includes(label)
                      ? "border-accent bg-accent/15 text-accent"
                      : "border-outline-strong bg-canvas-elevated text-transparent",
                  )}
                >
                  ✓
                </span>
                <div className="min-w-0 flex-1">
                  <p
                    className={cn(
                      "truncate text-[length:var(--text-sm)]",
                      index === activeOptionIndex || props.selected.includes(label)
                        ? "font-medium text-foreground"
                        : "text-foreground-muted",
                    )}
                  >
                    {label}
                  </p>
                  <p className="mt-0.5 text-[length:var(--text-xs)] text-foreground-subtle">
                    {props.selected.includes(label) ? "Selected" : "Available"}
                  </p>
                </div>
              </button>
            ))}
          </div>
          <p className="mt-3 font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
            j/k move  space toggle  enter apply  tab custom labels  esc close
          </p>
          <label
            className="mt-5 grid gap-2 border border-outline bg-canvas-elevated px-3 py-3"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            <span className="mono-meta">Custom labels</span>
            <span className="text-xs text-foreground-subtle">
              Optional. Add new labels or multiple labels separated by commas.
            </span>
            <input
              className="rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
              placeholder="Follow Up, Waiting"
              value={props.customLabel}
              onChange={(event) => props.onCustomLabelChange(event.target.value)}
            />
          </label>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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

function clampIndex(index: number, length: number) {
  if (length <= 0) {
    return 0;
  }
  return Math.min(Math.max(index, 0), length - 1);
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Move message
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Move the selected message into a different label or system mailbox.
          </Dialog.Description>
          <label className="mt-5 grid gap-2">
            <span className="mono-meta">Target</span>
            <select
              className="rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
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
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
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
                  "flex items-start gap-3 rounded-md border px-4 py-3 text-sm",
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
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(26rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
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
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-danger/30 bg-danger/10 px-4 py-2 text-sm text-danger"
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Go to label
          </Dialog.Title>
          <label className="mt-5 grid gap-2">
            <span className="mono-meta">Lens</span>
            <select
              className="rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
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
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(40rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Attachments
          </Dialog.Title>
          <div className="mt-5 space-y-3">
            {props.attachments.map((attachment) => (
              <div
                key={attachment.id}
                className="flex items-center justify-between gap-4 rounded-md border border-outline bg-panel-elevated px-4 py-4"
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(40rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Links in thread
          </Dialog.Title>
          <div className="mt-5 space-y-3">
            {props.links.map((link) => (
              <div
                key={link}
                className="flex items-center justify-between gap-4 rounded-md border border-outline bg-panel-elevated px-4 py-4"
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
        <Dialog.Popup className="fixed inset-6 z-40 overflow-hidden rounded-md border border-outline bg-panel outline-none">
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
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(44rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
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
                className="min-h-24 rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
                value={props.form.condition}
                onChange={(event) =>
                  props.onChange((current) => ({ ...current, condition: event.target.value }))
                }
              />
            </label>
            <label className="grid gap-2">
              <span className="mono-meta">Action</span>
              <textarea
                className="min-h-28 rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
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
                  className="rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground outline-none"
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
              <label className="flex items-center gap-3 rounded border border-outline bg-panel-elevated px-4 py-3 text-sm text-foreground-muted">
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
                className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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

export function SavedSearchDialog(props: {
  open: boolean;
  name: string;
  query: string;
  mode: string;
  onClose: () => void;
  onNameChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog.Root open={props.open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-40 w-[min(28rem,calc(100vw-3rem))] -translate-x-1/2 -translate-y-1/2 rounded-md border border-outline bg-panel px-6 py-6 outline-none">
          <Dialog.Title className="text-2xl font-semibold tracking-tight text-foreground">
            Save search
          </Dialog.Title>
          <Dialog.Description className="mt-3 text-sm text-foreground-muted">
            Save the current search as a sidebar lens for quick access.
          </Dialog.Description>
          <div className="mt-5 grid gap-4">
            <ComposeField
              label="Name"
              value={props.name}
              placeholder="My search"
              onChange={props.onNameChange}
            />
            {props.query ? (
              <div className="grid gap-2">
                <span className="mono-meta">Query</span>
                <p className="rounded border border-outline bg-panel-elevated px-4 py-3 font-mono text-sm text-foreground-muted">
                  {props.query}
                </p>
              </div>
            ) : null}
            <div className="grid gap-2">
              <span className="mono-meta">Mode</span>
              <p className="text-sm text-foreground-muted">{props.mode}</p>
            </div>
          </div>
          <div className="mt-6 flex justify-end gap-2">
            <button
              type="button"
              className="rounded border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
              onClick={props.onClose}
            >
              Cancel
            </button>
            <button
              type="button"
              className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
              onClick={props.onSubmit}
              disabled={!props.name.trim()}
            >
              Save
            </button>
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
        <Dialog.Popup className="fixed inset-6 z-40 overflow-hidden rounded-md border border-outline bg-panel outline-none">
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
                  className="min-h-full w-full rounded-md border border-outline bg-panel-muted px-5 py-5 font-mono text-sm leading-7 text-foreground outline-none"
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
                    className="rounded border border-accent/30 bg-accent/12 px-4 py-2 text-sm text-accent"
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
