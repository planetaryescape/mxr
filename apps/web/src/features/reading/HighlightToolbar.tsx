import { type RefObject, useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { Button } from "@/components/ui/button";
import { isComposingKey } from "@/lib/keys/composition";

import type { ReadingHighlight } from "./api";
import { paragraphOfRangeStart } from "./paragraphAnchor";
import { highlightMatching, isOwnHighlight } from "./readingView";
import { editHighlightNote, highlightSelection, removeHighlight } from "./readingVerbs";

interface Picked {
  /** The selected words, as the browser gives them. */
  text: string;
  /** Where the selection is on screen. */
  rect: DOMRect;
  /** The paragraph it starts in, for anchoring the highlight. */
  paragraph: number | null;
}

/**
 * The current text selection when it lies inside `containerRef`. A selection
 * in the highlights list, the header or anywhere else gives null, so the
 * toolbar only ever offers to highlight the article's own text.
 */
function useSelectionIn(containerRef: RefObject<HTMLElement | null>, paused: boolean) {
  const [picked, setPicked] = useState<Picked | null>(null);
  const read = useCallback(() => {
    const selection = window.getSelection();
    const container = containerRef.current;
    if (!selection || !container || selection.rangeCount === 0 || selection.isCollapsed) {
      setPicked(null);
      return;
    }
    const range = selection.getRangeAt(0);
    const text = selection.toString();
    if (!text.trim() || !container.contains(range.commonAncestorContainer)) {
      setPicked(null);
      return;
    }
    setPicked({
      text,
      rect: range.getBoundingClientRect(),
      paragraph: paragraphOfRangeStart(range),
    });
  }, [containerRef]);
  useEffect(() => {
    // While a note is typed the textarea takes the selection; the toolbar keeps the picked text.
    if (paused) return;
    document.addEventListener("selectionchange", read);
    // The reader column scrolls, so the selection moves under the toolbar.
    window.addEventListener("scroll", read, true);
    window.addEventListener("resize", read);
    return () => {
      document.removeEventListener("selectionchange", read);
      window.removeEventListener("scroll", read, true);
      window.removeEventListener("resize", read);
    };
  }, [paused, read]);
  return picked;
}

const EDGE = 8;

function anchorStyle(rect: DOMRect): React.CSSProperties {
  const above = rect.top > 64;
  const left = Math.min(Math.max(rect.left + rect.width / 2, 140), window.innerWidth - 140);
  return {
    position: "fixed",
    left,
    top: above ? rect.top - EDGE : rect.bottom + EDGE,
    transform: above ? "translate(-50%, -100%)" : "translateX(-50%)",
  };
}

/**
 * A small toolbar anchored to the selection. Selecting article text offers
 * Highlight and Highlight with note; selecting (or clicking) a saved
 * highlight offers its note and removal instead. A highlight made in
 * Marginalia is read-only here, and says where to change it.
 */
export function HighlightToolbar({
  containerRef,
  itemKey,
  view,
  highlights,
  onOpenMarginalia,
}: {
  containerRef: RefObject<HTMLElement | null>;
  itemKey: string;
  view: "issue" | "article";
  highlights: readonly ReadingHighlight[];
  onOpenMarginalia: () => void;
}) {
  const [composing, setComposing] = useState<Picked | null>(null);
  const picked = useSelectionIn(containerRef, composing !== null);
  const target = composing ?? picked;
  if (!target) return null;
  const existing = highlightMatching(target.text, highlights);
  return createPortal(
    <div
      role="toolbar"
      aria-label={existing ? "Highlight" : "Highlight the selection"}
      data-testid="highlight-toolbar"
      style={anchorStyle(target.rect)}
      // A press must not clear the selection it acts on.
      onMouseDown={(event) => {
        if (!(event.target instanceof HTMLTextAreaElement)) event.preventDefault();
      }}
      className="z-50 flex max-w-[min(92vw,360px)] flex-col gap-2 rounded-md border border-border bg-popover p-1.5 text-popover-foreground shadow-md"
    >
      {existing ? (
        <ExistingControls
          highlight={existing}
          itemKey={itemKey}
          onOpenMarginalia={onOpenMarginalia}
        />
      ) : composing ? (
        <NoteComposer
          onCancel={() => setComposing(null)}
          onSave={async (note) => {
            const saved = await highlightSelection(
              composing.text,
              itemKey,
              view,
              note,
              composing.paragraph,
            );
            if (saved) {
              setComposing(null);
              window.getSelection()?.removeAllRanges();
            }
          }}
        />
      ) : (
        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            data-testid="toolbar-highlight"
            onClick={async () => {
              if (
                await highlightSelection(target.text, itemKey, view, undefined, target.paragraph)
              ) {
                window.getSelection()?.removeAllRanges();
              }
            }}
          >
            Highlight
          </Button>
          <Button
            size="sm"
            variant="ghost"
            data-testid="toolbar-highlight-note"
            onClick={() => setComposing(target)}
          >
            Highlight with note
          </Button>
        </div>
      )}
    </div>,
    document.body,
  );
}

function ExistingControls({
  highlight,
  itemKey,
  onOpenMarginalia,
}: {
  highlight: ReadingHighlight;
  itemKey: string;
  onOpenMarginalia: () => void;
}) {
  const [editing, setEditing] = useState(false);
  if (!isOwnHighlight(highlight)) {
    return (
      <div className="flex flex-wrap items-center gap-2 px-1 text-xs">
        <span>Made in Marginalia; change it there</span>
        <Button size="sm" variant="ghost" onClick={onOpenMarginalia}>
          Open in Marginalia
        </Button>
      </div>
    );
  }
  if (editing) {
    return (
      <NoteComposer
        initial={highlight.note ?? ""}
        onCancel={() => setEditing(false)}
        onSave={async (note) => {
          if (await editHighlightNote(itemKey, highlight.id, note)) setEditing(false);
        }}
      />
    );
  }
  return (
    <div className="flex items-center gap-1">
      <Button
        size="sm"
        variant="ghost"
        data-testid="toolbar-edit-note"
        onClick={() => setEditing(true)}
      >
        {highlight.note ? "Edit note" : "Add note"}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        data-testid="toolbar-remove"
        onClick={() => {
          void removeHighlight(itemKey, highlight.id);
          window.getSelection()?.removeAllRanges();
        }}
      >
        Remove
      </Button>
    </div>
  );
}

/** A note field: Enter saves, Shift+Enter breaks the line, Esc cancels. A blank note clears it. */
export function NoteComposer({
  initial = "",
  onSave,
  onCancel,
}: {
  initial?: string;
  onSave: (note: string) => Promise<void> | void;
  onCancel: () => void;
}) {
  const [note, setNote] = useState(initial);
  const [saving, setSaving] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => field.current?.focus(), []);
  const save = async () => {
    if (saving) return;
    setSaving(true);
    try {
      await onSave(note);
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="grid gap-1.5">
      <textarea
        ref={field}
        value={note}
        rows={3}
        maxLength={2000}
        aria-label="Note"
        placeholder="Add a note"
        data-testid="highlight-note-input"
        onChange={(event) => setNote(event.target.value)}
        onKeyDown={(event) => {
          // The reader's single-letter keys must not fire while typing.
          event.stopPropagation();
          if (event.key === "Escape") onCancel();
          if (event.key === "Enter" && !event.shiftKey && !isComposingKey(event)) {
            event.preventDefault();
            void save();
          }
        }}
        className="w-full min-w-[240px] resize-none rounded-md border border-border bg-background p-2 text-sm text-foreground"
      />
      <div className="flex justify-end gap-1">
        <Button size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          size="sm"
          data-testid="highlight-note-save"
          disabled={saving}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
    </div>
  );
}
