import { markdown } from "@codemirror/lang-markdown";
import { Compartment, EditorState, Prec, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, keymap } from "@codemirror/view";
import { getCM, Vim, vim } from "@replit/codemirror-vim";
import { basicSetup } from "codemirror";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  codeMirrorKey,
  composeCommandOf,
  isComposeChord,
} from "@/features/compose/session/composeShortcuts";
import { definedShortcut } from "@/lib/actions/definition";
import type { ModelSpan } from "@/features/compose/assist/modelSpans";

interface CodeMirrorComposeEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  onSend: () => void;
  onDiscard: () => void;
  onClose?: () => void;
  autoFocus?: boolean;
  quiet?: boolean;
  inline?: boolean;
  onSelectionChange?: (selection: { from: number; to: number } | null) => void;
  modelSpans?: readonly ModelSpan[];
  focusNonce?: number;
  readOnly?: boolean;
}

const setModelSpans = StateEffect.define<readonly ModelSpan[]>();

const modelSpanField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, transaction) {
    for (const effect of transaction.effects) {
      if (!effect.is(setModelSpans)) continue;
      const length = transaction.state.doc.length;
      return Decoration.set(
        effect.value
          .filter((span) => span.start >= 0 && span.start < span.end && span.end <= length)
          .map((span) => modelSpanMark.range(span.start, span.end)),
        true,
      );
    }
    return marks.map(transaction.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

const modelSpanMark = Decoration.mark({ class: "cm-model-span" });

const tokenTheme = EditorView.theme({
  ".cm-model-span": {
    textDecoration: "underline dotted var(--text-3)",
    textDecorationThickness: "1px",
    textUnderlineOffset: "4px",
  },
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
    minHeight: "100%",
    padding: "20px 20px 56px 20px",
    fontFamily: "var(--font-mono)",
    caretColor: "var(--foreground)",
  },
  ".cm-gutters": { display: "none" },
  ".cm-activeLine": { backgroundColor: "transparent" },
  ".cm-cursor": { borderLeftColor: "var(--foreground)" },
  "&.cm-focused": { outline: "none" },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in oklab, var(--primary) 24%, transparent) !important",
  },
});

const inlineTheme = EditorView.theme({
  "&": { height: "auto" },
  ".cm-editor": { height: "auto" },
  ".cm-scroller": { overflow: "visible" },
  ".cm-content": { minHeight: "320px" },
});

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
  inline = false,
  onSelectionChange,
  modelSpans,
  focusNonce = 0,
  readOnly = false,
}: CodeMirrorComposeEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [inlineLayout] = useState(() => new Compartment());
  const [editableLayout] = useState(() => new Compartment());
  const readOnlyRef = useRef(readOnly);
  readOnlyRef.current = readOnly;
  const initialInlineRef = useRef(inline);
  const initialValueRef = useRef(value);
  const initialSpansRef = useRef(modelSpans ?? []);
  const callbacksRef = useRef({ onChange, onSave, onSend, onDiscard, onClose, onSelectionChange });
  callbacksRef.current = { onChange, onSave, onSend, onDiscard, onClose, onSelectionChange };
  const [vimMode, setVimMode] = useState("normal");

  useEffect(() => {
    if (!hostRef.current) return;
    let lastSelection: { from: number; to: number } | null = null;
    const reportSelection = (state: EditorState) => {
      const main = state.selection.main;
      const next = main.empty ? null : { from: main.from, to: main.to };
      if (next?.from === lastSelection?.from && next?.to === lastSelection?.to) return;
      lastSelection = next;
      callbacksRef.current.onSelectionChange?.(next);
    };
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
          Prec.highest(
            EditorView.domEventHandlers({
              keydown: (event) => {
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
          vimCursorTheme,
          vim(),
          tokenTheme,
          inlineLayout.of(initialInlineRef.current ? inlineTheme : []),
          editableLayout.of([
            EditorView.editable.of(!readOnlyRef.current),
            EditorState.readOnly.of(readOnlyRef.current),
          ]),
          keymap.of([
            {
              key: codeMirrorKey(definedShortcut("compose.save")),
              run: () => {
                callbacksRef.current.onSave();
                return true;
              },
            },
          ]),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({
            "aria-label": "Message body",
            "aria-multiline": "true",
          }),
          modelSpanField.init(() =>
            Decoration.set(
              initialSpansRef.current
                .filter(
                  (span) =>
                    span.start >= 0 &&
                    span.start < span.end &&
                    span.end <= initialValueRef.current.length,
                )
                .map((span) => modelSpanMark.range(span.start, span.end)),
              true,
            ),
          ),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) callbacksRef.current.onChange(update.state.doc.toString());
            if (update.docChanged || update.selectionSet) reportSelection(update.state);
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
  }, [autoFocus, editableLayout, inlineLayout]);

  useLayoutEffect(() => {
    const view = viewRef.current;
    if (view) view.dispatch({ effects: inlineLayout.reconfigure(inline ? inlineTheme : []) });
  }, [inline, inlineLayout]);

  useLayoutEffect(() => {
    const view = viewRef.current;
    if (view)
      view.dispatch({
        effects: editableLayout.reconfigure([
          EditorView.editable.of(!readOnly),
          EditorState.readOnly.of(readOnly),
        ]),
      });
  }, [editableLayout, readOnly]);

  useEffect(() => {
    if (focusNonce > 0) viewRef.current?.focus();
  }, [focusNonce]);

  const appliedSpans = useRef<readonly ModelSpan[] | undefined>(undefined);
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    const textChanged = value !== current;
    const marksChanged = appliedSpans.current !== modelSpans;
    if (!textChanged && !marksChanged) return;
    appliedSpans.current = modelSpans;
    view.dispatch({
      ...(textChanged ? { changes: { from: 0, to: current.length, insert: value } } : {}),
      ...(marksChanged ? { effects: setModelSpans.of(modelSpans ?? []) } : {}),
    });
  }, [value, modelSpans]);

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <div ref={hostRef} className="min-h-0 flex-1" />
      {quiet ? (
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

function isSendChord(event: KeyboardEvent): boolean {
  return composeCommandOf(event) === "requestSend" && !event.altKey;
}
