import { Loader2, Paperclip, Send } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import type { ComposeEditor } from "@/state/uiPrefsStore";
import { DraftQualityBadges } from "./DraftQualityBadges";
import type { DraftSuggestionResponse } from "./types";

interface ComposeActionBarProps {
  onSend: () => void;
  onAttach: () => void;
  uploading: number;
  busy: boolean;
  saveStatus: string;
  dirty: boolean;
  saveError: string | null;
  editorPreference: ComposeEditor;
  onEditorChange: (editor: ComposeEditor) => void;
  suggestion: DraftSuggestionResponse | null;
}

export function ComposeActionBar({
  onSend,
  onAttach,
  uploading,
  busy,
  saveStatus,
  dirty,
  saveError,
  editorPreference,
  onEditorChange,
  suggestion,
}: ComposeActionBarProps) {
  return (
    <footer className="shrink-0 border-t border-border bg-card/30">
      <div className="mx-auto flex h-14 w-full max-w-[860px] items-center gap-2 px-5">
        <Button type="button" onClick={onSend} disabled={busy} className="gap-2">
          <Send className="size-4" />
          Send
          <kbd className="ml-0.5 rounded border border-primary-foreground/25 bg-primary-foreground/10 px-1 py-0.5 font-mono text-[10px] leading-none">
            ⌘↵
          </kbd>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onAttach}
          disabled={uploading > 0}
          className="gap-1.5"
        >
          {uploading > 0 ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Paperclip className="size-3.5" />
          )}
          Attach
        </Button>
        <ToggleGroup
          type="single"
          value={editorPreference}
          onValueChange={(value) => value && onEditorChange(value as ComposeEditor)}
          aria-label="Editor mode"
        >
          <ToggleGroupItem value="tiptap" size="sm" className="px-2.5 text-2xs">
            Rich text
          </ToggleGroupItem>
          <ToggleGroupItem value="codemirror-vim" size="sm" className="px-2.5 text-2xs">
            Markdown
          </ToggleGroupItem>
        </ToggleGroup>
        <div className="ml-auto flex min-w-0 items-center gap-3">
          <DraftQualityBadges suggestion={suggestion} compact />
          {saveError ? (
            <span className="truncate text-2xs text-destructive" title={saveError}>
              {saveError}
            </span>
          ) : (
            <span className={cn("text-2xs", dirty ? "text-warning" : "text-success")}>
              {saveStatus}
            </span>
          )}
        </div>
      </div>
    </footer>
  );
}
