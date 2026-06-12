import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { getCM, Vim, vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import { useEffect, useRef, useState } from "react";

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
    padding: "24px 24px 56px 16px",
    fontFamily: "var(--font-mono)",
    caretColor: "var(--primary)",
  },
  ".cm-gutters": {
    backgroundColor: "var(--background)",
    color: "var(--muted-foreground)",
    borderRightColor: "transparent",
  },
  ".cm-cursor": { borderLeftColor: "var(--primary)" },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in oklch, var(--primary) 24%, transparent) !important",
  },
});

export function CodeMirrorComposeEditor({
  value,
  onChange,
  onSave,
  onSend,
  onDiscard,
  onClose,
  autoFocus = false,
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
          basicSetup,
          markdown(),
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
              key: "Mod-Enter",
              run: () => {
                callbacksRef.current.onSend();
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
    <div className="flex h-full min-h-0 flex-col">
      <div ref={hostRef} className="min-h-0 flex-1" />
      <div
        aria-live="polite"
        className="flex h-5 shrink-0 items-center justify-between border-t border-border/60 px-3 font-mono text-2xs uppercase tracking-wide text-muted-foreground"
      >
        <span data-testid="vim-mode">-- {vimMode} --</span>
        <span className="normal-case tracking-normal">:w save · :q close · :wq both</span>
      </div>
    </div>
  );
}
