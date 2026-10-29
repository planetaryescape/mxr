import {
  ArrowLeft,
  Check,
  ListTodo,
  MessageSquareReply,
  Reply,
  ReplyAll,
  ThumbsUp,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { ModeMarks } from "@/components/ModeMark";
import { ReasonLine } from "@/components/ReasonLine";
import { otherModes, useThreadModes } from "@/features/modes/membership";
import { OtherAccountLine } from "@/features/thread/OtherAccountLine";
import { cn } from "@/lib/utils";

import { mergePeople, refreshMessages, type MergeSuggestion, type PersonPage } from "./api";

import { ConversationView } from "./Conversation";
import { ConversationPeople } from "./ConversationPeople";
import { GotItBar } from "./GotItBar";
import { type Arrival } from "./messagesView";
import { PersonIdentity } from "./PersonIdentity";
import { TopicSwitcher } from "./TopicSwitcher";
import type { PendingAck } from "./useGotIt";

export interface PersonPaneProps {
  page: PersonPage;
  topic: string | null;
  /** Topics on this page still in Messages. */
  left: number;
  /** Threads whose done is on its way: they fold out of the topics. */
  hidden: ReadonlySet<string>;
  /** What a verb just moved to on this page, marked once. */
  arrival: Arrival | null;
  /** The page shown is the previous topic's while this one loads. */
  loading: boolean;
  asSent: string | null;
  replyAll: boolean;
  pending: PendingAck | null;
  ackLoading: boolean;
  onBack: () => void;
  onTopic: (threadId: string) => void;
  onToggleAsSent: (messageId: string) => void;
  onReplyAllChange: (on: boolean) => void;
  onReply: () => void;
  onGotIt: () => void;
  /** Got it gained focus or the pointer: its hint's moment of need. */
  onGotItFocus: () => void;
  onUndoGotIt: () => void;
  onDone: () => void;
  onTodo: () => void;
  /** Hints anchored under the topic list and above the footer's verbs. */
  topicsHint?: React.ReactNode;
  gotItHint?: React.ReactNode;
}

/**
 * One person's page: how you know them, every topic with them (groups as
 * "with Ruth: …"), the selected topic as a conversation, and the reply box
 * at the bottom, already addressed.
 */
export function PersonPane(props: PersonPaneProps) {
  const { page } = props;
  const row = page.row;
  const conversation = page.conversation ?? null;
  const selected = conversation?.thread_id ?? props.topic;
  const draft = conversation && row.draft?.thread_id === conversation.thread_id ? row.draft : null;
  const replying = useComposeUi(
    (s) =>
      Boolean(conversation && s.intent?.contextThreadId === conversation.thread_id) &&
      s.surface === "inline",
  );
  const composeOpenForConversation = useComposeUi((s) =>
    Boolean(conversation && s.intent?.contextThreadId === conversation.thread_id),
  );
  const modes = useThreadModes(conversation?.thread_id, conversation?.account_id).data;
  const readingArea = useRef<HTMLDivElement>(null);
  const scrolledThread = useRef<string | null>(null);
  useEffect(() => {
    const threadId = conversation?.thread_id;
    if (!threadId || props.loading || scrolledThread.current === threadId) return;
    if (replying) {
      scrolledThread.current = threadId;
      return;
    }
    const frame = requestAnimationFrame(() => {
      const area = readingArea.current;
      const latest = area?.querySelector<HTMLElement>('[data-latest="true"]');
      if (!area || !latest) return;
      const areaTop = area.getBoundingClientRect().top;
      const latestTop = latest.getBoundingClientRect().top;
      area.scrollTop = Math.max(0, area.scrollTop + latestTop - areaTop);
      scrolledThread.current = threadId;
    });
    return () => cancelAnimationFrame(frame);
  }, [conversation?.thread_id, props.loading, replying]);
  return (
    <section
      aria-label={row.title}
      data-testid="person-page"
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background"
    >
      <PageFrame
        width="reader"
        detail
        publishTitle={false}
        titleTestId="person-name"
        purposeTestId="relationship-line"
        eyebrow={
          <button
            type="button"
            onClick={props.onBack}
            aria-label="Back to the people"
            className="inline-flex items-center gap-1 hover:text-text-1 md:hidden"
          >
            <ArrowLeft className="size-3" /> People
          </button>
        }
        title={row.title}
        titleContent={<PersonIdentity row={row} interactive={!props.loading} />}
        purpose={page.relationship_line}
        freshness={<span data-testid="person-header-line">{page.header_line}</span>}
      />
      {/* A phone has no status bar, so the reason shows on the person's own page. */}
      <ReasonLine reason={row.why} className="px-5 pt-2 sm:hidden" />
      {props.left > 0 || (page.merge_suggestions ?? []).length > 0 ? (
        <ModeFrame width="reader" className="px-5 pt-2">
          {props.left > 0 ? (
            <p data-testid="topics-left" className="type-meta text-text-3">
              {props.left === 1 ? "1 topic" : `${props.left} topics`} left with {row.title}
            </p>
          ) : null}
          {(page.merge_suggestions ?? []).map((suggestion) => (
            <MergeLine key={suggestion.addresses.join(",")} suggestion={suggestion} />
          ))}
        </ModeFrame>
      ) : null}
      <TopicSwitcher
        topics={page.topics}
        selected={selected}
        hidden={props.hidden}
        arrival={props.arrival}
        onTopic={props.onTopic}
        hint={props.topicsHint}
      />
      {conversation && !props.loading ? (
        <ModeFrame width="reader" className="shrink-0">
          <ConversationPeople
            key={conversation.thread_id}
            threadId={conversation.thread_id}
            accountId={conversation.account_id}
            trailing={<ModeMarks modes={otherModes(modes, "messages")} />}
          />
        </ModeFrame>
      ) : null}
      <div
        ref={readingArea}
        data-testid="conversation-reading-area"
        aria-busy={props.loading || undefined}
        className={cn(
          "min-h-0 flex-1 overflow-y-auto transition-opacity duration-fast",
          props.loading && "opacity-40",
        )}
      >
        <ModeFrame width="reader">
          {conversation ? (
            <>
              <OtherAccountLine accountId={conversation.account_id} />
              <ConversationView
                key={props.topic ?? conversation.thread_id}
                conversation={conversation}
                asSent={props.asSent}
                onToggleAsSent={props.onToggleAsSent}
                disclosuresEnabled={!props.loading}
              />
            </>
          ) : (
            <p className="px-5 py-6 text-sm text-muted-foreground">No conversation to show.</p>
          )}
          {/* ComposeHost portals the reply here, as it does in the reader. */}
          <div
            id={
              conversation
                ? `inline-composer-slot-${conversation.thread_id}`
                : "inline-composer-slot"
            }
            className="px-4 pb-4 empty:hidden"
          />
        </ModeFrame>
      </div>
      {conversation ? (
        <footer className="shrink-0 border-t border-border pt-3">
          <ModeFrame width="reader">
            {props.pending ? <GotItBar pending={props.pending} onUndo={props.onUndoGotIt} /> : null}
            {!replying ? (
              <div className="px-4">
                <button
                  type="button"
                  data-testid="composer"
                  onClick={props.onReply}
                  className="flex w-full items-center gap-2 rounded-none border border-rule bg-ink-lamp px-4 py-2.5 text-left text-sm text-text-2 hover:border-text-3"
                >
                  <MessageSquareReply className="size-4 shrink-0" aria-hidden />
                  {draft ? (
                    // The reply you left here, ready to carry on.
                    <span className="min-w-0 flex-1 truncate text-text-1">
                      <span className="font-medium text-signal">Draft:</span> {draft.text}
                    </span>
                  ) : (
                    <span className="min-w-0 flex-1 truncate">{conversation.composer.label}</span>
                  )}
                  <KeyChip className="shrink-0">r</KeyChip>
                </button>
              </div>
            ) : null}
            {props.gotItHint ? <div className="px-4 pt-2">{props.gotItHint}</div> : null}
            <div className="flex flex-wrap items-center gap-x-1 gap-y-1 px-3 pb-3 pt-2 text-xs">
              {!composeOpenForConversation ? (
                <div
                  role="group"
                  aria-label="Reply mode"
                  className="mr-2 inline-flex items-center gap-1"
                >
                  <button
                    type="button"
                    aria-pressed={!props.replyAll}
                    onClick={() => props.onReplyAllChange(false)}
                    className="inline-flex items-center gap-1 rounded-sm px-2 py-1 text-text-2 hover:bg-ink-lamp aria-pressed:bg-ink-lamp aria-pressed:text-text-1"
                  >
                    <Reply className="size-3.5" aria-hidden /> Reply
                  </button>
                  <button
                    type="button"
                    aria-pressed={props.replyAll}
                    onClick={() => props.onReplyAllChange(true)}
                    className="inline-flex items-center gap-1 rounded-sm px-2 py-1 text-text-2 hover:bg-ink-lamp aria-pressed:bg-ink-lamp aria-pressed:text-text-1"
                  >
                    <ReplyAll className="size-3.5" aria-hidden /> Reply all
                  </button>
                </div>
              ) : null}
              <FooterButton
                testId="got-it"
                onClick={props.onGotIt}
                onNeed={props.onGotItFocus}
                disabled={props.ackLoading || props.pending !== null}
                icon={<ThumbsUp className="size-3.5" aria-hidden />}
                label="Got it"
                chip="."
              />
              <FooterButton
                testId="done-here"
                onClick={props.onDone}
                icon={<Check className="size-3.5" aria-hidden />}
                label="Done here"
                chip="e"
              />
              <FooterButton
                testId="make-todo"
                onClick={props.onTodo}
                icon={<ListTodo className="size-3.5" aria-hidden />}
                label="To do"
                chip="t"
              />
            </div>
          </ModeFrame>
        </footer>
      ) : null}
    </section>
  );
}

function FooterButton({
  testId,
  onClick,
  onNeed,
  disabled,
  icon,
  label,
  chip,
}: {
  testId: string;
  onClick: () => void;
  /** Focus or pointer arrived: about to be used. */
  onNeed?: () => void;
  disabled?: boolean;
  icon: React.ReactNode;
  label: string;
  chip: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      onFocus={onNeed}
      onPointerEnter={onNeed}
      disabled={disabled}
      className="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-foreground/90 hover:bg-accent disabled:opacity-50"
    >
      {icon}
      {label}
      <KeyChip className="hidden h-4 px-1 md:inline-flex">{chip}</KeyChip>
    </button>
  );
}

/** "Same person? Samir Patel (2 addresses)": previewed, then merged on confirm. */
function MergeLine({ suggestion }: { suggestion: MergeSuggestion }) {
  const [preview, setPreview] = useState<string | null>(null);
  const [into, ...rest] = suggestion.addresses;
  if (!into) return null;
  const run = async (dryRun: boolean) => {
    try {
      const merge = await mergePeople(suggestion.account_id, into, rest, dryRun);
      if (dryRun) {
        setPreview(merge.summary);
      } else {
        toast.success(merge.summary);
        setPreview(null);
        await refreshMessages();
      }
    } catch (error) {
      toast.error("Couldn't merge", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };
  return (
    <p
      data-testid="merge-suggestion"
      className="mt-2 flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground"
    >
      <span>
        Same person? {suggestion.name} ({suggestion.addresses.length} addresses)
      </span>
      {preview ? (
        <>
          <span className="text-foreground">{preview}</span>
          <button
            type="button"
            onClick={() => void run(false)}
            className="text-foreground underline"
          >
            Merge
          </button>
          <button type="button" onClick={() => setPreview(null)} className="hover:text-foreground">
            Cancel
          </button>
        </>
      ) : (
        <button type="button" onClick={() => void run(true)} className="text-foreground underline">
          Merge…
        </button>
      )}
    </p>
  );
}
