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
  /** Close the composer chrome (`:q` / `:wq`); optional for hosts without
   * a closable surface. */
  onClose?: () => void;
  autoFocus?: boolean;
  /** Focus mode: a one-word mode indicator instead of the status row. */
  quiet?: boolean;
  /** Inline replies grow with their text so the conversation scrolls. */
  inline?: boolean;
  /** The selection in the body (UTF-16), or null when there is none. */
  onSelectionChange?: (selection: { from: number; to: number } | null) => void;
  /** Ranges the model wrote that the user has not edited; drawn with a hairline. */
  modelSpans?: readonly ModelSpan[];
  /** Changes when the editor should take focus back, e.g. after a proof closes. */
  focusNonce?: number;
  readOnly?: boolean;
}

const setModelSpans = StateEffect.define<readonly ModelSpan[]>();

/**
 * Hairline marks under model-written text. The spans are owned by the
 * draft, which maps them through each edit; this only draws them, clipped
 * to the document so a stale range can't throw.
 */
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
  // The whole writing area is the editor: a click anywhere in it lands the caret.
  ".cm-content": {
    minHeight: "100%",
    padding: "20px 20px 56px 20px",
    fontFamily: "var(--font-mono)",
    caretColor: "var(--foreground)",
  },
  // Line numbers mean nothing in an email; the vim status line is enough.
  ".cm-gutters": { display: "none" },
  // An email is prose, not code: a highlighted line made an empty body read as a one-line strip.
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
    // Only a real selection is reported; a caret moving is not a change the
    // compose surface acts on, and reporting it re-renders the whole panel.
    let lastSelection: { from: number; to: number } | null = null;
    const reportSelection = (state: EditorState) => {
      const main = state.selection.main;
      const next = main.empty ? null : { from: main.from, to: main.to };
      if (next?.from === lastSelection?.from && next?.to === lastSelection?.to) return;
      lastSelection = next;
      callbacksRef.current.onSelectionChange?.(next);
    };
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

  // Each change of `focusNonce` asks for the editor back (after a proof).
  useEffect(() => {
    if (focusNonce > 0) viewRef.current?.focus();
  }, [focusNonce]);

  // The document and its marks change in one transaction. Marks installed
  // first would be checked against the old document, where text that is
  // only in the new one is out of range, and be dropped.
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

/** The send chord (⌘Enter or Ctrl+Enter) without Alt. With Shift it is
 * send and archive, which the compose surface handles. */
function isSendChord(event: KeyboardEvent): boolean {
  return composeCommandOf(event) === "requestSend" && !event.altKey;
}
