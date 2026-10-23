import { ArrowLeft, Check, ListTodo, MessageSquareReply, ThumbsUp } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { ArrivalGlow } from "@/components/ArrivalGlow";
import { KeyChip } from "@/components/KeyChip";
import { ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { ModeMarks } from "@/components/ModeMark";
import { ReasonLine } from "@/components/ReasonLine";
import { otherModes, useThreadModes } from "@/features/modes/membership";
import { cn } from "@/lib/utils";

import { mergePeople, refreshMessages, type MergeSuggestion, type PersonPage } from "./api";
import { ConversationView } from "./Conversation";
import { GotItBar } from "./GotItBar";
import { topicLabel, topicStateLabel, type Arrival } from "./messagesView";
import { PersonIdentity } from "./PersonIdentity";
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
  const replying = useComposeUi((s) => s.intent !== null && s.surface === "inline");
  const modes = useThreadModes(conversation?.thread_id).data;
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
      <nav aria-label="Topics" className="shrink-0 border-b border-border">
        <ModeFrame width="reader" className="px-3 py-2">
          <p className="type-eyebrow px-2 pb-1 text-text-3">Topics</p>
          <ul data-testid="topics" className="flex max-h-[28vh] flex-col gap-0.5 overflow-y-auto">
            {page.topics.map((topic) => {
              const active = topic.thread_id === selected;
              const leaving = props.hidden.has(topic.thread_id);
              // A topic arrival marks that topic; a person arrival, the one open.
              const arrived = props.arrival
                ? (props.arrival.thread ?? selected) === topic.thread_id
                : false;
              return (
                <li
                  key={topic.thread_id}
                  data-leaving={leaving ? "true" : undefined}
                  className={cn(leaving && "todo-fold pointer-events-none opacity-60")}
                >
                  <button
                    type="button"
                    data-testid="topic"
                    aria-current={active ? "true" : undefined}
                    onClick={() => props.onTopic(topic.thread_id)}
                    className={cn(
                      "type-body relative flex w-full items-baseline gap-3 px-2 py-1 text-left",
                      active ? "bg-ink-lamp text-text-1" : "text-text-2 hover:text-text-1",
                    )}
                  >
                    {active ? (
                      <span aria-hidden className="absolute inset-y-0 left-0 w-[2px] bg-signal" />
                    ) : null}
                    {arrived ? <ArrivalGlow key={props.arrival?.key} /> : null}
                    <span className="min-w-0 flex-1 truncate">
                      {topic.state === "your_turn" ? (
                        <span
                          aria-hidden
                          data-testid="topic-turn"
                          className="mr-2 inline-block size-1.5 bg-cue align-middle"
                        />
                      ) : null}
                      {topicLabel(topic)}
                    </span>
                    <span className="type-meta shrink-0 text-text-3">{topicStateLabel(topic)}</span>
                  </button>
                </li>
              );
            })}
          </ul>
          <p className="type-meta hidden px-2 pt-1 text-text-3 md:block">
            <KeyChip className="h-4 px-1">[</KeyChip> <KeyChip className="h-4 px-1">]</KeyChip> step
            through topics
          </p>
          {props.topicsHint ? <div className="px-2">{props.topicsHint}</div> : null}
        </ModeFrame>
      </nav>
      <div
        aria-busy={props.loading || undefined}
        className={cn(
          "min-h-0 flex-1 overflow-y-auto transition-opacity duration-fast",
          props.loading && "opacity-40",
        )}
      >
        <ModeFrame width="reader">
          {conversation ? (
            <>
              <div className="mx-3 mt-3 flex items-center justify-between gap-3 px-2 py-1">
                <h3
                  data-testid="conversation-subject"
                  className="type-heading relative min-w-0 truncate text-text-1"
                >
                  {props.arrival &&
                  (props.arrival.thread ?? conversation.thread_id) === conversation.thread_id ? (
                    <ArrivalGlow key={props.arrival.key} />
                  ) : null}
                  {conversation.subject}
                </h3>
                <ModeMarks modes={otherModes(modes, "messages")} />
              </div>
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
          <div id="inline-composer-slot" className="px-4 pb-4 empty:hidden" />
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
                  <span className="min-w-0 flex-1 truncate">{conversation.composer.label}</span>
                  <KeyChip className="shrink-0">r</KeyChip>
                </button>
              </div>
            ) : null}
            {props.gotItHint ? <div className="px-4 pt-2">{props.gotItHint}</div> : null}
            <div className="flex flex-wrap items-center gap-x-1 gap-y-1 px-3 pb-3 pt-2 text-xs">
              <label className="mr-2 inline-flex items-center gap-1.5 px-1 text-muted-foreground">
                <input
                  type="checkbox"
                  checked={props.replyAll}
                  onChange={(event) => props.onReplyAllChange(event.target.checked)}
                  className="accent-primary"
                />
                Reply all
              </label>
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
