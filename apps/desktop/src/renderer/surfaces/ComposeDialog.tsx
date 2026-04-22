import { Dialog } from "@base-ui/react";
import Editor, { type OnMount } from "@monaco-editor/react";
import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { ComposeFrontmatter, ComposeSession, UtilityRailPayload } from "../../shared/types";
import { cn } from "../lib/cn";
import { composeTitle } from "./formatters";

export function ComposeDialog(props: {
  open: boolean;
  session: ComposeSession | null;
  draft: ComposeFrontmatter | null;
  busyLabel: string | null;
  error: string | null;
  utilityRail: UtilityRailPayload;
  knownSenders: Array<{ name: string; email: string }>;
  onDraftChange: Dispatch<SetStateAction<ComposeFrontmatter | null>>;
  onClose: () => void;
  onOpenEditor: () => void;
  onAttachFiles: () => void;
  onRemoveAttachment: (path: string) => void;
  onRefresh: () => void;
  onSend: () => void;
  onSave: () => void;
  onDiscard: () => void;
  onPersistDraft: () => Promise<void>;
  onBodyChange: (body: string) => void;
  fetchContactSuggestions: (query: string) => Promise<Array<{ label: string; value: string }>>;
}) {
  const open = props.open && Boolean(props.session && props.draft);
  const toRef = useRef<HTMLInputElement>(null);
  const [body, setBody] = useState("");
  const [showCc, setShowCc] = useState(false);

  // Contact picker state
  const [contactQuery, setContactQuery] = useState("");
  const [contactIndex, setContactIndex] = useState(0);
  const [showContacts, setShowContacts] = useState(false);

  // Initialize body from session
  useEffect(() => {
    if (open && props.session) {
      setBody(props.session.bodyMarkdown || "");
      setShowCc(Boolean(props.draft?.cc || props.draft?.bcc));
      setContactQuery("");
      setShowContacts(false);
      setTimeout(() => toRef.current?.focus(), 50);
    }
  }, [open]);

  // Parse recipients
  const recipients = (props.draft?.to || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  // Filter contacts
  const filteredContacts = useMemo(() => {
    if (!contactQuery || contactQuery.length < 1) return [];
    const q = contactQuery.toLowerCase();
    return props.knownSenders
      .filter((c) => c.name.toLowerCase().includes(q) || c.email.toLowerCase().includes(q))
      .filter((c) => !recipients.includes(c.email))
      .slice(0, 8);
  }, [contactQuery, props.knownSenders, recipients]);

  useEffect(() => { setContactIndex(0); }, [contactQuery]);

  const addRecipient = (email: string) => {
    const current = recipients.filter(Boolean);
    if (!current.includes(email)) current.push(email);
    props.onDraftChange((c) => c ? { ...c, to: current.join(", ") } : c);
    setContactQuery("");
    setShowContacts(false);
    toRef.current?.focus();
  };

  const handleToKeyDown = (e: React.KeyboardEvent) => {
    if (showContacts && filteredContacts.length > 0) {
      if (e.key === "ArrowDown") { e.preventDefault(); setContactIndex((i) => Math.min(i + 1, filteredContacts.length - 1)); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); setContactIndex((i) => Math.max(i - 1, 0)); return; }
      if (e.key === "Tab" || e.key === "Enter") { e.preventDefault(); addRecipient(filteredContacts[contactIndex].email); return; }
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (contactQuery.includes("@")) {
        addRecipient(contactQuery);
      }
      return;
    }
    if (e.key === "Backspace" && !contactQuery && recipients.length > 0) {
      e.preventDefault();
      const updated = recipients.slice(0, -1);
      props.onDraftChange((c) => c ? { ...c, to: updated.join(", ") } : c);
    }
    if (e.key === "Escape") { setShowContacts(false); }
  };

  const handleToChange = (value: string) => {
    setContactQuery(value);
    setShowContacts(value.length >= 1);
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => { if (!next) props.onClose(); }}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-30 bg-canvas/72 backdrop-blur-sm" />
        <Dialog.Popup
          className="fixed inset-4 z-40 flex flex-col overflow-hidden border border-outline bg-panel outline-none"
          style={{ borderRadius: "var(--radius-md)" }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); props.onSend(); }
            if (e.key === "s" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); props.onSave(); }
          }}
        >
          {props.session && props.draft ? (
            <>
              {/* Header */}
              <div className="flex shrink-0 items-center justify-between gap-3 border-b border-outline px-4 py-2">
                <Dialog.Title className="text-[length:var(--text-sm)] font-medium text-foreground">
                  {composeTitle(props.session.kind)}
                </Dialog.Title>
                <div className="flex items-center gap-3 text-[length:var(--text-2xs)] text-foreground-subtle">
                  {props.error ? <span className="text-danger">{props.error}</span> : null}
                  {props.busyLabel ? <span className="text-warning">{props.busyLabel}</span> : null}
                  <span>
                    <kbd className="font-mono text-foreground-muted">Tab</kbd> next
                    <span className="mx-1">·</span>
                    <kbd className="font-mono text-foreground-muted">Shift+Tab</kbd> fields
                    <span className="mx-1">·</span>
                    <kbd className="font-mono text-foreground-muted">Ctrl+Enter</kbd> send
                    <span className="mx-1">·</span>
                    <kbd className="font-mono text-foreground-muted">Esc</kbd> close
                  </span>
                </div>
              </div>

              {/* To field with contact picker */}
              <div className="relative flex shrink-0 items-center gap-3 border-b border-outline/50 px-4 py-2">
                <span className="w-14 shrink-0 text-right text-[length:var(--text-xs)] text-foreground-subtle">To</span>
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
                  {recipients.map((email) => (
                    <span key={email} className="bg-accent/12 px-2 py-0.5 text-[length:var(--text-xs)] text-accent" style={{ borderRadius: "var(--radius-sm)" }}>
                      {email}
                    </span>
                  ))}
                  <input
                    ref={toRef}
                    className="min-w-[10rem] flex-1 bg-transparent text-[length:var(--text-sm)] text-foreground outline-none placeholder:text-foreground-subtle"
                    value={contactQuery}
                    placeholder={recipients.length === 0 ? "Type a name or email..." : ""}
                    onChange={(e) => handleToChange(e.target.value)}
                    onKeyDown={handleToKeyDown}
                    onFocus={() => { if (contactQuery) setShowContacts(true); }}
                    onBlur={() => setTimeout(() => setShowContacts(false), 150)}
                  />
                </div>
                {!showCc ? (
                  <button type="button" tabIndex={-1} className="shrink-0 text-[length:var(--text-xs)] text-foreground-subtle hover:text-accent" onClick={() => setShowCc(true)}>
                    Cc
                  </button>
                ) : null}

                {/* Contact dropdown */}
                {showContacts && filteredContacts.length > 0 ? (
                  <div className="absolute left-16 right-4 top-full z-20 mt-1 max-h-48 overflow-y-auto border border-outline bg-panel shadow-xl" style={{ borderRadius: "var(--radius-sm)" }}>
                    {filteredContacts.map((c, i) => (
                      <button
                        key={c.email}
                        type="button"
                        className={cn(
                          "flex w-full items-center gap-3 px-3 py-1.5 text-left",
                          i === contactIndex ? "bg-panel-elevated text-foreground" : "text-foreground-muted hover:bg-panel-elevated/50",
                        )}
                        onMouseDown={(e) => { e.preventDefault(); addRecipient(c.email); }}
                        onMouseEnter={() => setContactIndex(i)}
                      >
                        <span className="truncate text-[length:var(--text-sm)]">{c.name || c.email}</span>
                        {c.name ? <span className="truncate text-[length:var(--text-xs)] text-foreground-subtle">{c.email}</span> : null}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>

              {/* Cc/Bcc */}
              {showCc ? (
                <>
                  <FieldRow label="Cc" value={props.draft.cc} onChange={(v) => props.onDraftChange((c) => c ? { ...c, cc: v } : c)} />
                  <FieldRow label="Bcc" value={props.draft.bcc} onChange={(v) => props.onDraftChange((c) => c ? { ...c, bcc: v } : c)} />
                </>
              ) : null}

              {/* Subject */}
              <FieldRow label="Subject" value={props.draft.subject} onChange={(v) => props.onDraftChange((c) => c ? { ...c, subject: v } : c)} placeholder="Subject" />

              <div className="flex shrink-0 items-start gap-3 border-b border-outline/50 px-4 py-2">
                <span className="w-14 shrink-0 pt-1 text-right text-[length:var(--text-xs)] text-foreground-subtle">
                  Attach
                </span>
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                  <button
                    type="button"
                    className="border border-outline bg-panel-elevated px-2 py-1 text-[length:var(--text-xs)] text-foreground-muted hover:text-foreground"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    onClick={props.onAttachFiles}
                  >
                    Attach files
                  </button>
                  {props.draft.attach.map((path) => {
                    const label = attachmentLabel(path);
                    return (
                      <span
                        key={path}
                        className="flex items-center gap-1.5 bg-accent/12 px-2 py-0.5 text-[length:var(--text-xs)] text-accent"
                        style={{ borderRadius: "var(--radius-sm)" }}
                      >
                        <span className="truncate">{label}</span>
                        <button
                          type="button"
                          className="text-accent/70 hover:text-accent"
                          aria-label={`Remove attachment ${label}`}
                          onClick={() => props.onRemoveAttachment(path)}
                        >
                          ×
                        </button>
                      </span>
                    );
                  })}
                </div>
              </div>

              {/* Body editor with vim motions */}
              <div className="min-h-0 flex-1 border-t border-outline">
                <VimEditor
                  value={body}
                  onChange={(v) => { setBody(v); props.onBodyChange(v); }}
                  onFocusFields={() => toRef.current?.focus()}
                />
              </div>
              {/* Vim status line */}
              <div id="vim-status" className="shrink-0 border-t border-outline/50 px-4 py-0.5 font-mono text-[length:var(--text-2xs)] text-foreground-subtle" />

              {/* Footer */}
              <div className="flex shrink-0 items-center justify-between border-t border-outline px-4 py-2">
                <span className="font-mono text-[length:var(--text-xs)] text-foreground-subtle">{props.draft.from}</span>
                <div className="flex items-center gap-1.5">
                  <button type="button" className="px-2 py-1 text-[length:var(--text-xs)] text-foreground-subtle hover:text-foreground" onClick={props.onOpenEditor}>
                    Open in $EDITOR
                  </button>
                  <button type="button" className="px-2 py-1 text-[length:var(--text-xs)] text-foreground-subtle hover:text-danger" onClick={props.onDiscard}>
                    Discard
                  </button>
                  <button type="button" className="px-2 py-1 text-[length:var(--text-xs)] text-foreground-muted hover:text-foreground" onClick={props.onSave}>
                    Save
                  </button>
                  <button
                    type="button"
                    className="bg-accent/12 px-3 py-1 text-[length:var(--text-xs)] font-medium text-accent hover:bg-accent/20"
                    style={{ borderRadius: "var(--radius-sm)" }}
                    onClick={props.onSend}
                  >
                    Send
                  </button>
                </div>
              </div>
            </>
          ) : null}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function FieldRow(props: { label: string; value: string; placeholder?: string; onChange: (v: string) => void }) {
  return (
    <label className="flex shrink-0 items-center gap-3 border-b border-outline/50 px-4 py-2">
      <span className="w-14 shrink-0 text-right text-[length:var(--text-xs)] text-foreground-subtle">{props.label}</span>
      <input
        className="min-w-0 flex-1 bg-transparent text-[length:var(--text-sm)] text-foreground outline-none placeholder:text-foreground-subtle"
        value={props.value}
        placeholder={props.placeholder}
        onChange={(e) => props.onChange(e.target.value)}
      />
    </label>
  );
}

function attachmentLabel(path: string) {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function VimEditor(props: { value: string; onChange: (value: string) => void; onFocusFields?: () => void }) {
  const vimRef = useRef<{ dispose: () => void } | null>(null);

  const handleMount: OnMount = useCallback((editor, monaco) => {
    // Shift+Tab moves focus back to fields
    editor.addAction({
      id: "mxr.focusFields",
      label: "Focus compose fields",
      keybindings: [monaco.KeyMod.Shift | monaco.KeyCode.Tab],
      run: () => { props.onFocusFields?.(); },
    });
    // Define mxr dark theme
    monaco.editor.defineTheme("mxr-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": "#0f1520",
        "editor.foreground": "#edf3ff",
        "editor.lineHighlightBackground": "#182132",
        "editor.selectionBackground": "#293852",
        "editorCursor.foreground": "#6ab7ff",
        "editorLineNumber.foreground": "#4a5568",
        "editorLineNumber.activeForeground": "#8091ab",
      },
    });
    monaco.editor.setTheme("mxr-dark");

    // Enable vim mode
    import("monaco-vim").then((monacoVim) => {
      const statusEl = document.getElementById("vim-status");
      if (statusEl) {
        const vim = monacoVim.initVimMode(editor, statusEl);
        vimRef.current = vim;
      }
    });

    editor.focus();
  }, []);

  useEffect(() => {
    return () => {
      vimRef.current?.dispose();
      vimRef.current = null;
    };
  }, []);

  return (
    <Editor
      height="100%"
      language="markdown"
      value={props.value}
      onChange={(value) => props.onChange(value ?? "")}
      onMount={handleMount}
      options={{
        minimap: { enabled: false },
        wordWrap: "on",
        lineNumbers: "on",
        fontSize: 13,
        fontFamily: "IBM Plex Mono, monospace",
        scrollBeyondLastLine: false,
        renderLineHighlight: "line",
        overviewRulerBorder: false,
        hideCursorInOverviewRuler: true,
        scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
        padding: { top: 12, bottom: 12 },
        cursorBlinking: "smooth",
        cursorSmoothCaretAnimation: "on",
      }}
    />
  );
}
