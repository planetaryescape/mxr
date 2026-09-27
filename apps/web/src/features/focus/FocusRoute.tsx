/*
 * Focus & reply: everyone you owe a reply, one conversation at a time. The
 * conversation and what it asks of you on the left, your reply on the
 * right; send and the next one is up, skip it for later, or snooze it.
 * Progress is a count and a thin bar; the end says what's next.
 */

import { useNavigate } from "@tanstack/react-router";
import { MessageSquareReply } from "lucide-react";
import { useEffect, useRef } from "react";

import { Button } from "@/components/ui/button";
import { KeyChip } from "@/components/KeyChip";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { targetFromThread } from "@/features/mail-actions/target";
import type { ThreadResponse } from "@/features/mailbox/types";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { runCommand, useScopeController } from "@/lib/keys/controllers";
import { getActiveQueryClient } from "@/lib/queryClient";

import { FocusFinish } from "./FocusFinish";
import { FocusThread } from "./FocusThread";
import { focusReplyIntent, useFocusSession } from "./useFocusSession";

export function FocusRoute({ from }: { from?: string }) {
  const navigate = useNavigate();
  const focus = useFocusSession();
  const { current, progress } = focus;
  const replyKey = current ? focusReplyIntent(current).key : null;
  const replyOpen = useComposeUi(
    (s) => s.intent !== null && s.intent.key === replyKey && s.surface === "inline",
  );
  const pageRef = useRef<HTMLDivElement>(null);
  const replyRef = useRef<HTMLDivElement>(null);
  useShortcutScope("focus");

  const leave = () => void navigate({ href: safeReturnPath(from) });
  /** The open reply's commands, only while it is this conversation's. */
  const replyCommands = () => {
    const commands = useComposeUi.getState().commands;
    return commands && commands.intentKey === replyKey ? commands : null;
  };
  const snooze = () => {
    if (!current) return;
    const thread = getActiveQueryClient()?.getQueryData<ThreadResponse>([
      "thread",
      current.threadId,
    ]);
    if (!thread) return;
    const threadId = current.threadId;
    openMailDialog({
      kind: "snooze",
      target: targetFromThread(thread),
      onDone: () => focus.markHandled(threadId),
    });
  };
  const focusReply = () =>
    replyRef.current?.querySelector<HTMLElement>(".cm-content, .ProseMirror")?.focus();

  useScopeController("focus", {
    send: () => replyCommands()?.send(),
    skip: focus.skip,
    snooze,
    remind: () => replyCommands()?.sendAndRemind(),
    draft: () => replyCommands()?.draftForMe(),
    reply: () => (replyOpen ? focusReply() : focus.reopenReply()),
    leave,
  });

  // Esc inside the reply steps out to focus mode's keys (skip, snooze)
  // instead of leaving. The composer is portaled here, so this listens on
  // the DOM; an editor that uses Esc itself (vim mode) keeps it.
  useEffect(() => {
    const node = replyRef.current;
    if (!node) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (!(event.target instanceof HTMLElement)) return;
      if (!event.target.closest("[data-compose-surface]")) return;
      event.preventDefault();
      event.target.blur();
      pageRef.current?.focus({ preventScroll: true });
    };
    node.addEventListener("keydown", onKeyDown);
    return () => node.removeEventListener("keydown", onKeyDown);
  }, []);

  const finished = !focus.loading && !current;

  return (
    <div
      ref={pageRef}
      tabIndex={-1}
      aria-label="Focus and reply"
      // Tab skips only from the page itself (where Esc leaves you); from a
      // button or link it still moves focus.
      onKeyDown={(event) => {
        if (event.key !== "Tab" || event.shiftKey || event.target !== event.currentTarget) return;
        event.preventDefault();
        focus.skip();
      }}
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background outline-none"
    >
      <header className="shrink-0 border-b border-border">
        <div className="flex h-11 items-center gap-3 px-5">
          <span className="hidden font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground sm:inline">
            Focus & reply
          </span>
          {current ? (
            <span
              className="min-w-0 truncate text-[13px] tabular-nums"
              data-testid="focus-progress"
            >
              <span className="text-foreground">
                {progress.position} of {progress.total}
              </span>
              <span className="text-muted-foreground"> · {current.sender}</span>
            </span>
          ) : null}
          {focus.gathering && current ? (
            <span className="text-2xs text-muted-foreground" role="status">
              Finding everyone else you owe…
            </span>
          ) : null}
          <Button variant="ghost" size="sm" onClick={leave} className="ml-auto gap-2">
            Leave <KeyChip className="h-4 px-1">esc</KeyChip>
          </Button>
        </div>
        <div
          role="progressbar"
          aria-label="Replies done"
          aria-valuemin={0}
          aria-valuemax={progress.total}
          aria-valuenow={progress.done}
          className="h-0.5 bg-border/60"
        >
          <div
            data-testid="focus-progress-bar"
            className="h-full origin-left bg-primary transition-transform duration-base ease-out"
            style={{ transform: `scaleX(${progress.total ? progress.done / progress.total : 0})` }}
          />
        </div>
      </header>

      {focus.error && !current && !focus.loading ? (
        <p className="px-5 py-3 text-xs text-destructive">
          Couldn't load everyone you owe: {focus.error.message}
        </p>
      ) : null}

      {finished ? (
        <FocusFinish replied={progress.done} empty={progress.total === 0} onLeave={leave} />
      ) : (
        <div className="grid min-h-0 flex-1 grid-rows-[auto_auto] overflow-y-auto md:grid-cols-2 md:grid-rows-1 md:overflow-hidden">
          <section aria-label="Conversation" className="min-w-0 md:min-h-0 md:overflow-y-auto">
            {current ? (
              <>
                <div className="px-5 pt-4">
                  <h1 className="text-balance text-[17px] font-semibold leading-snug tracking-tight">
                    {current.subject || "(no subject)"}
                  </h1>
                  <p className="mt-0.5 text-xs text-muted-foreground tabular-nums">
                    {current.reason}
                  </p>
                </div>
                <FocusThread key={current.threadId} threadId={current.threadId} />
              </>
            ) : (
              <div className="h-40 animate-pulse" aria-busy="true" />
            )}
          </section>
          <section
            aria-label="Your reply"
            className="flex min-w-0 flex-col border-t border-border md:min-h-0 md:overflow-y-auto md:border-l md:border-t-0"
          >
            <div ref={replyRef} className="flex min-h-0 flex-1 flex-col px-4 py-4">
              {/* ComposeHost portals the reply here, as it does in the reader.
                  In focus mode the reply takes the whole column. */}
              <div
                id="inline-composer-slot"
                className="flex min-h-[320px] flex-1 flex-col empty:hidden [&_[data-compose-surface]]:max-h-none [&_[data-compose-surface]]:flex-1"
              />
              {current && !replyOpen ? (
                <button
                  type="button"
                  onClick={focus.reopenReply}
                  className="flex w-full items-center gap-2 rounded-lg border border-border bg-surface/60 px-4 py-3 text-left text-[13.5px] text-muted-foreground hover:border-border-strong"
                >
                  <MessageSquareReply className="size-4" aria-hidden />
                  Reply to {current.sender}…<KeyChip className="ml-auto">r</KeyChip>
                </button>
              ) : null}
            </div>
            <FocusKeys disabled={!current} run={(command) => runCommand("focus", command)} />
          </section>
        </div>
      )}
    </div>
  );
}

/**
 * The ways through the queue, on screen as buttons with their keys. In the
 * reply, ⌘↵ sends; Tab (or Esc in the rich-text editor) steps out of it so
 * the single keys work.
 */
function FocusKeys({
  disabled,
  run,
}: {
  disabled: boolean;
  run: (command: "send" | "skip" | "snooze" | "remind" | "draft") => void;
}) {
  const keys = [
    ["send", "⌘↵", "Send and next"],
    ["skip", "s", "Skip"],
    ["snooze", "Z", "Snooze"],
    ["remind", "w", "Send, remind if no reply"],
    ["draft", "d", "Draft in your voice"],
  ] as const;
  return (
    <div
      role="toolbar"
      aria-label="Move through the queue"
      className="flex shrink-0 flex-wrap items-center gap-x-1 gap-y-1 border-t border-border px-3 py-2"
    >
      {keys.map(([command, key, label]) => (
        <button
          key={command}
          type="button"
          disabled={disabled}
          onClick={() => run(command)}
          className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11.5px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50"
        >
          <KeyChip className="h-4 px-1">{key}</KeyChip>
          {label}
        </button>
      ))}
    </div>
  );
}

/** Only a path inside the app, never another origin. */
export function safeReturnPath(from: string | undefined): string {
  if (!from || !from.startsWith("/") || from.startsWith("//") || from.startsWith("/focus")) {
    return "/m/inbox";
  }
  return from;
}
