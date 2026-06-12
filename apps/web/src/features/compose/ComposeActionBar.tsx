import { Clock, Loader2, Paperclip, Send } from "lucide-react";

import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import type { ComposeEditor } from "@/state/uiPrefsStore";
import { DraftQualityBadges } from "./DraftQualityBadges";
import type { DraftSuggestionResponse } from "./types";

interface ComposeActionBarProps {
  onSend: () => void;
  onSendLater: () => void;
  onAttach: () => void;
  uploading: number;
  busy: boolean;
  saveStatus: string;
  dirty: boolean;
  saveError: string | null;
  onRetrySave: () => void;
  editorPreference: ComposeEditor;
  onEditorChange: (editor: ComposeEditor) => void;
  suggestion: DraftSuggestionResponse | null;
}

export function ComposeActionBar({
  onSend,
  onSendLater,
  onAttach,
  uploading,
  busy,
  saveStatus,
  dirty,
  saveError,
  onRetrySave,
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
          size="icon-sm"
          onClick={onSendLater}
          disabled={busy}
          aria-label="Send later"
          title="Send later (⇧⌘L)"
        >
          <Clock className="size-3.5" />
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
            <span role="alert" className="flex min-w-0 items-center gap-1.5">
              <span className="truncate text-2xs font-medium text-destructive" title={saveError}>
                Not saved — {saveError}
              </span>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onRetrySave}
                disabled={busy}
                className="h-6 px-2 text-2xs"
              >
                Retry
              </Button>
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
