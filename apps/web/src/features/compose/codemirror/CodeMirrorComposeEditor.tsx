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

  return <div ref={hostRef} className="h-full min-h-0" />;
}
