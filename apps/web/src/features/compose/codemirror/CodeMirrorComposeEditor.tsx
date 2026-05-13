import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";

interface CodeMirrorComposeEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  onSend: () => void;
  onDiscard: () => void;
  autoFocus?: boolean;
}

const tokenTheme = EditorView.theme({
  "&": {
    minHeight: "420px",
    backgroundColor: "hsl(var(--surface))",
    color: "var(--foreground)",
    fontSize: "13px",
  },
  ".cm-content": {
    minHeight: "420px",
    fontFamily: "var(--font-mono)",
    padding: "18px",
  },
  ".cm-gutters": {
    backgroundColor: "hsl(var(--surface))",
    color: "var(--muted-foreground)",
    borderRightColor: "var(--border)",
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
  autoFocus = false,
}: CodeMirrorComposeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const initialValueRef = useRef(value);
  const callbacksRef = useRef({ onChange, onSave, onSend, onDiscard });
  callbacksRef.current = { onChange, onSave, onSend, onDiscard };

  useEffect(() => {
    if (!hostRef.current) return;
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
    <div ref={hostRef} className="overflow-hidden rounded-xl border border-border bg-surface" />
  );
}
