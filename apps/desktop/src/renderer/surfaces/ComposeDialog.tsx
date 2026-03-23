import { Dialog } from "@base-ui/react";
import type { Dispatch, SetStateAction } from "react";
import type { ComposeFrontmatter, ComposeSession, UtilityRailPayload } from "../../shared/types";
import { cn } from "../lib/cn";
import { composeKindLabel, composeTitle, escapeHtml } from "./formatters";

export function ComposeDialog(props: {
  open: boolean;
  session: ComposeSession | null;
  draft: ComposeFrontmatter | null;
  busyLabel: string | null;
  error: string | null;
  utilityRail: UtilityRailPayload;
  onDraftChange: Dispatch<SetStateAction<ComposeFrontmatter | null>>;
  onClose: () => void;
  onOpenEditor: () => void;
  onRefresh: () => void;
  onSend: () => void;
  onSave: () => void;
  onDiscard: () => void;
}) {
  const open = props.open && Boolean(props.session && props.draft);
  const attachments = props.draft?.attach.join(", ") ?? "";

  return (
      <Dialog.Root open={open} onOpenChange={(next) => !next && props.onClose()}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup className="fixed inset-4 z-40 overflow-hidden border border-outline bg-panel outline-none">
          {props.session && props.draft ? (
            <div className="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[minmax(0,1.5fr)_18rem]">
              <section className="flex min-h-0 flex-col">
                <div className="border-b border-outline px-4 py-4">
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <p className="mono-meta">{composeKindLabel(props.session.kind)}</p>
                      <Dialog.Title className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
                        {composeTitle(props.session.kind)}
                      </Dialog.Title>
                      <Dialog.Description className="mt-2 text-sm leading-6 text-foreground-muted">
                        Body stays in {props.session.editorCommand}. Desktop owns recipients,
                        preview, send, and draft state.
                      </Dialog.Description>
                    </div>
                    <button
                      type="button"
                      className="border border-outline bg-panel-elevated px-2 py-1.5 text-xs uppercase text-foreground-muted"
                      onClick={props.onClose}
                    >
                      Close
                    </button>
                  </div>
                  {props.error ? (
                    <div className="mt-3 border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
                      {props.error}
                    </div>
                  ) : null}
                  <div className="mt-4 grid gap-2 md:grid-cols-2">
                    <ComposeField
                      label="To"
                      value={props.draft.to}
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current ? { ...current, to: value } : current,
                        )
                      }
                    />
                    <ComposeField
                      label="Subject"
                      value={props.draft.subject}
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current ? { ...current, subject: value } : current,
                        )
                      }
                    />
                    <ComposeField
                      label="Cc"
                      value={props.draft.cc}
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current ? { ...current, cc: value } : current,
                        )
                      }
                    />
                    <ComposeField
                      label="Bcc"
                      value={props.draft.bcc}
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current ? { ...current, bcc: value } : current,
                        )
                      }
                    />
                    <ComposeField
                      label="From"
                      value={props.draft.from}
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current ? { ...current, from: value } : current,
                        )
                      }
                    />
                    <ComposeField
                      label="Attachments"
                      value={attachments}
                      placeholder="/tmp/file.pdf, /tmp/spec.md"
                      onChange={(value) =>
                        props.onDraftChange((current) =>
                          current
                            ? {
                                ...current,
                                attach: value
                                  .split(",")
                                  .map((item) => item.trim())
                                  .filter(Boolean),
                              }
                            : current,
                        )
                      }
                    />
                  </div>
                  {props.session.issues.length > 0 ? (
                    <div className="mt-3 flex flex-wrap gap-1.5">
                      {props.session.issues.map((issue) => (
                        <span
                          key={`${issue.severity}-${issue.message}`}
                          className={cn(
                            "border px-2 py-0.5 text-[11px]",
                            issue.severity === "error"
                              ? "border-danger/30 bg-danger/10 text-danger"
                              : "border-warning/30 bg-warning/10 text-warning",
                          )}
                        >
                          {issue.message}
                        </span>
                      ))}
                    </div>
                  ) : null}
                </div>

                <div className="subtle-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-4">
                  <div className="border border-outline bg-panel-muted p-4">
                    <p className="mono-meta">Preview</p>
                    <div
                      className="prose prose-invert mt-3 max-w-none text-foreground-muted [&_a]:text-accent [&_blockquote]:border-outline-strong [&_code]:bg-panel [&_code]:px-1 [&_code]:py-0.5"
                      dangerouslySetInnerHTML={{
                        __html:
                          props.session.previewHtml ||
                          `<p>${escapeHtml(
                            props.session.bodyMarkdown ||
                              "Open the draft in your editor to start writing.",
                          )}</p>`,
                      }}
                    />
                  </div>
                </div>

                <div className="flex flex-wrap items-center justify-between gap-3 border-t border-outline px-4 py-3">
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      className="border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
                      onClick={props.onOpenEditor}
                    >
                      Open in editor
                    </button>
                    <button
                      type="button"
                      className="border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
                      onClick={props.onRefresh}
                    >
                      Refresh
                    </button>
                  </div>
                  <div className="flex items-center gap-2">
                    {props.busyLabel ? (
                      <span className="mono-meta text-warning">{props.busyLabel}</span>
                    ) : null}
                    <button
                      type="button"
                      className="border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
                      onClick={props.onDiscard}
                    >
                      Discard
                    </button>
                    <button
                      type="button"
                      className="border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground"
                      onClick={props.onSave}
                    >
                      Save draft
                    </button>
                    <button
                      type="button"
                      className="border border-accent/30 bg-accent/12 px-3 py-1.5 text-xs uppercase text-accent"
                      onClick={props.onSend}
                    >
                      Send
                    </button>
                  </div>
                </div>
              </section>

              <aside className="hidden border-l border-outline bg-panel-muted px-4 py-4 lg:block">
                <p className="mono-meta">{props.utilityRail.title}</p>
                <div className="mt-3 space-y-px">
                  {props.utilityRail.items.map((item) => (
                    <div
                      key={item}
                      className="border border-outline bg-panel px-3 py-2 text-sm text-foreground-muted"
                    >
                      {item}
                    </div>
                  ))}
                </div>
                <div className="mt-4 border border-outline bg-panel px-3 py-3">
                  <p className="mono-meta">Draft file</p>
                  <p className="mt-3 break-all text-xs leading-6 text-foreground-muted">
                    {props.session.draftPath}
                  </p>
                </div>
              </aside>
            </div>
          ) : null}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

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
        className="border border-outline bg-panel-elevated px-3 py-2 text-sm text-foreground outline-none placeholder:text-foreground-subtle"
        value={props.value}
        placeholder={props.placeholder}
        onChange={(event) => props.onChange(event.target.value)}
      />
    </label>
  );
}
