import { Archive, BellRing, ChevronDown, Clock, Loader2, Paperclip, Send } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { describeChoice } from "@/features/time/api";
import { useResolvedPresets } from "@/features/time/useNaturalTime";
import { cn } from "@/lib/utils";
import type { ComposeEditor } from "@/state/uiPrefsStore";
import { DraftQualityBadges } from "./DraftQualityBadges";
import type { DraftSuggestionResponse } from "./types";

/** "Send and remind me if no reply in..." presets (TUI `n` parity). */
const REMIND_PRESETS = [
  { label: "1 day", input: "in 1 day" },
  { label: "3 days", input: "in 3 days" },
  { label: "1 week", input: "in 7 days" },
] as const;

interface ComposeActionBarProps {
  onSend: () => void;
  onSendLater: () => void;
  /** Present only for replies: a new message has no conversation to archive. */
  onSendAndArchive?: () => void;
  onSendAndRemind: (at: Date, label: string) => void;
  onSendAndRemindCustom: () => void;
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
  onSendAndArchive,
  onSendAndRemind,
  onSendAndRemindCustom,
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
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              disabled={busy}
              aria-label="More send options"
              title="More send options"
            >
              <ChevronDown className="size-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-64">
            <DropdownMenuItem onSelect={() => onSendLater()}>
              <Clock className="size-3.5" />
              Send later...
              <DropdownMenuShortcut>⇧⌘L</DropdownMenuShortcut>
            </DropdownMenuItem>
            {onSendAndArchive ? (
              <DropdownMenuItem onSelect={() => onSendAndArchive()}>
                <Archive className="size-3.5" />
                Send and archive
                <DropdownMenuShortcut>⇧⌘↵</DropdownMenuShortcut>
              </DropdownMenuItem>
            ) : null}
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <BellRing className="size-3.5" />
                Send and remind me if no reply in
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                <RemindPresetItems onSendAndRemind={onSendAndRemind} />
                <DropdownMenuItem onSelect={() => onSendAndRemindCustom()}>
                  Custom...
                </DropdownMenuItem>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          </DropdownMenuContent>
        </DropdownMenu>
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
                Not saved: {saveError}
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

/**
 * The remind presets with the exact time each resolves to. Mounted only while
 * the submenu is open, so the daemon is asked only then; choosing one sends
 * the instant shown.
 */
function RemindPresetItems({
  onSendAndRemind,
}: {
  onSendAndRemind: (at: Date, label: string) => void;
}) {
  const times = useResolvedPresets(REMIND_PRESETS);
  return REMIND_PRESETS.map((preset, index) => {
    const { choice = null, failed = false } = times[index] ?? {};
    return (
      <DropdownMenuItem
        key={preset.input}
        disabled={!choice}
        onSelect={() => {
          if (choice) onSendAndRemind(new Date(choice.at), describeChoice(choice));
        }}
      >
        {preset.label}
        <DropdownMenuShortcut className="font-mono tracking-normal tabular-nums">
          {choice ? describeChoice(choice) : failed ? "Unavailable" : "…"}
        </DropdownMenuShortcut>
      </DropdownMenuItem>
    );
  });
}
