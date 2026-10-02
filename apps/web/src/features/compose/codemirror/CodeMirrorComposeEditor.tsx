import { markdown } from "@codemirror/lang-markdown";
import { EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { getCM, Vim, vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import { useEffect, useRef, useState } from "react";

import { isComposeChord } from "@/features/compose/session/composeShortcuts";

interface CodeMirrorComposeEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  onSend: () => void;
  onDiscard: () => void;
  /** Close the composer chrome (`:q` / `:wq`); optional for hosts without
   * a closable surface. */
  onClose?: () => void;
  autoFocus?: boolean;
  /** Focus mode: a one-word mode indicator instead of the status row. */
  quiet?: boolean;
}

const tokenTheme = EditorView.theme({
  "&": {
    height: "100%",
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
    fontSize: "15px",
  },
  ".cm-editor": { height: "100%" },
  ".cm-scroller": {
    overflow: "auto",
    lineHeight: "1.65",
    fontFamily: "var(--font-mono)",
  },
  ".cm-content": {
    maxWidth: "720px",
    padding: "20px 24px 56px 24px",
    fontFamily: "var(--font-mono)",
    caretColor: "var(--foreground)",
  },
  // Line numbers mean nothing in an email; the vim status line is enough.
  ".cm-gutters": { display: "none" },
  // Mixed in oklab: oklch takes transparent's missing hue as 0, which tints
  // the line pink and reads as a validation error.
  ".cm-activeLine": { backgroundColor: "color-mix(in oklab, var(--muted) 55%, transparent)" },
  ".cm-cursor": { borderLeftColor: "var(--foreground)" },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in oklab, var(--primary) 24%, transparent) !important",
  },
});

// The vim block cursor ships hard-coded red at the highest precedence; only
// a theme at that precedence replaces it with the theme's own ink.
const vimCursorTheme = Prec.highest(
  EditorView.theme({
    ".cm-fat-cursor": {
      background: "color-mix(in oklab, var(--foreground) 35%, transparent)",
    },
    "&:not(.cm-focused) .cm-fat-cursor": {
      background: "none",
      outline: "solid 1px color-mix(in oklab, var(--foreground) 45%, transparent)",
    },
  }),
);

export function CodeMirrorComposeEditor({
  value,
  onChange,
  onSave,
  onSend,
  onDiscard,
  onClose,
  autoFocus = false,
  quiet = false,
}: CodeMirrorComposeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialValueRef = useRef(value);
  const callbacksRef = useRef({ onChange, onSave, onSend, onDiscard, onClose });
  callbacksRef.current = { onChange, onSave, onSend, onDiscard, onClose };
  const [vimMode, setVimMode] = useState("normal");

  useEffect(() => {
    if (!hostRef.current) return;
    // Ex commands live in a global vim registry; redefining on mount keeps
    // them routed at the latest callbacks via callbacksRef.
    Vim.defineEx("write", "w", () => {
      callbacksRef.current.onSave();
    });
    Vim.defineEx("quit", "q", () => {
      callbacksRef.current.onClose?.();
    });
    const saveAndClose = () => {
      callbacksRef.current.onSave();
      callbacksRef.current.onClose?.();
    };
    Vim.defineEx("wq", "wq", saveAndClose);
    Vim.defineEx("xit", "x", saveAndClose);
    const view = new EditorView({
      parent: hostRef.current,
      state: EditorState.create({
        doc: initialValueRef.current,
        extensions: [
          // Ahead of vim, which takes Ctrl-Enter in insert mode: off macOS
          // (Mod is Ctrl) the send key must still send.
          Prec.highest(
            EditorView.domEventHandlers({
              keydown: (event) => {
                // Held send, save and discard chords run once; the keymap
                // below never sees their repeats.
                if (event.repeat && isComposeChord(event)) {
                  event.preventDefault();
                  return true;
                }
                if (!isSendChord(event)) return false;
                event.preventDefault();
                callbacksRef.current.onSend();
                return true;
              },
            }),
          ),
          basicSetup,
          markdown(),
          // Ahead of vim(): between equal precedences the earlier theme wins.
          vimCursorTheme,
          vim(),
          tokenTheme,
          keymap.of([
            {
              key: "Mod-s",
              run: () => {
                callbacksRef.current.onSave();
                return true;
              },
            },
            {
              key: "Mod-Backspace",
              run: () => {
                callbacksRef.current.onDiscard();
                return true;
              },
            },
          ]),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({
            "aria-label": "Message body",
            "aria-multiline": "true",
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) callbacksRef.current.onChange(update.state.doc.toString());
          }),
        ],
      }),
    });
    viewRef.current = view;
    const cm = getCM(view);
    cm?.on("vim-mode-change", (event: { mode: string; subMode?: string }) => {
      setVimMode(event.subMode ? `${event.mode} ${event.subMode}` : event.mode);
    });
    if (autoFocus) window.setTimeout(() => view.focus(), 0);
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [autoFocus]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (value !== current) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <div ref={hostRef} className="min-h-0 flex-1" />
      {quiet ? (
        // Focus mode: the mode as one small word, nothing else.
        <span
          aria-live="polite"
          data-testid="vim-mode"
          className="pointer-events-none absolute bottom-1.5 right-3 font-mono text-2xs lowercase text-muted-foreground/80"
        >
          {vimMode}
        </span>
      ) : (
        <div
          aria-live="polite"
          className="flex h-5 shrink-0 items-center justify-between border-t border-border/60 px-3 font-mono text-2xs uppercase tracking-wide text-muted-foreground"
        >
          <span data-testid="vim-mode">-- {vimMode} --</span>
          <span className="normal-case tracking-normal">:w save · :q close · :wq both</span>
        </div>
      )}
    </div>
  );
}

/** ⌘Enter or Ctrl+Enter alone: send. With Shift it is send and archive,
 * which the compose surface handles. */
function isSendChord(event: KeyboardEvent): boolean {
  return (
    event.key === "Enter" && (event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey
  );
}
