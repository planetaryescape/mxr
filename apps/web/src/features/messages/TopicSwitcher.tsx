import { ChevronDown } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";

import { ArrivalGlow } from "@/components/ArrivalGlow";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { KeyChip } from "@/components/KeyChip";
import { ModeFrame } from "@/components/ModeFrame";
import { pressedOverlayKey } from "@/lib/keys/overlayKeys";
import { cn } from "@/lib/utils";

import type { PersonPage } from "./api";
import { topicLabel, topicStateLabel, type Arrival } from "./messagesView";

export function TopicSwitcher({
  topics,
  selected,
  hidden,
  arrival,
  onTopic,
  hint,
}: {
  topics: PersonPage["topics"];
  selected: string | null | undefined;
  hidden: ReadonlySet<string>;
  arrival: Arrival | null;
  onTopic: (threadId: string) => void;
  hint?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const activeTopic = topics.find((topic) => topic.thread_id === selected);
  const selectedLabel = activeTopic
    ? activeTopic.with?.length
      ? topicLabel(activeTopic)
      : activeTopic.shape === "group"
        ? `Group: ${activeTopic.subject}`
        : activeTopic.subject
    : "Choose a conversation";
  const selectedArrived =
    arrival !== null && (arrival.thread ?? selected) === activeTopic?.thread_id;

  return (
    <Collapsible open={open} onOpenChange={setOpen} asChild>
      <nav aria-label="Topics" className="shrink-0 border-b border-border">
        <ModeFrame width="reader" className="px-3 py-1.5">
          <div className="flex items-center gap-2">
            <CollapsibleTrigger asChild>
              <button
                ref={trigger}
                type="button"
                aria-label={`Choose conversation, ${selectedLabel}, ${activeTopic?.shape === "group" ? "group" : "direct"}, ${topics.findIndex((topic) => topic.thread_id === selected) + 1} of ${topics.length}`}
                data-testid="topics-trigger"
                className="inline-flex min-w-0 flex-1 items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm text-text-2 hover:bg-ink-lamp focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-signal"
              >
                <span className="relative min-w-0 flex-1 truncate">
                  {selectedArrived ? <ArrivalGlow key={arrival?.key} /> : null}
                  <span className="relative">{selectedLabel}</span>
                </span>
                <span className="type-meta shrink-0 text-text-3">
                  {topics.findIndex((topic) => topic.thread_id === selected) + 1} of {topics.length}
                </span>
                <ChevronDown
                  className={cn("size-3.5 shrink-0", open && "rotate-180")}
                  aria-hidden
                />
              </button>
            </CollapsibleTrigger>
            <KeyChip className="hidden shrink-0 sm:inline-flex">{"{ }"}</KeyChip>
          </div>
          <CollapsibleContent>
            <ul
              data-testid="topics"
              className="mt-1 flex max-h-[45dvh] flex-col gap-0.5 overflow-y-auto"
              onKeyDown={(event) => {
                if (!pressedOverlayKey(event, "popover.close")) return;
                event.preventDefault();
                setOpen(false);
                requestAnimationFrame(() => trigger.current?.focus());
              }}
            >
              {topics.map((topic) => {
                const active = topic.thread_id === selected;
                const leaving = hidden.has(topic.thread_id);
                const arrived = arrival ? (arrival.thread ?? selected) === topic.thread_id : false;
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
                      onClick={() => {
                        onTopic(topic.thread_id);
                        setOpen(false);
                        requestAnimationFrame(() => trigger.current?.focus());
                      }}
                      className={cn(
                        "type-body relative flex w-full items-baseline gap-3 px-2 py-1 text-left",
                        active ? "bg-ink-lamp text-text-1" : "text-text-2 hover:text-text-1",
                      )}
                    >
                      {active ? (
                        <span aria-hidden className="absolute inset-y-0 left-0 w-[2px] bg-signal" />
                      ) : null}
                      {arrived ? <ArrivalGlow key={arrival?.key} /> : null}
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
                      <span className="type-meta shrink-0 text-text-3">
                        {topicStateLabel(topic)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
            {hint ? <div className="px-2 pt-1">{hint}</div> : null}
          </CollapsibleContent>
        </ModeFrame>
      </nav>
    </Collapsible>
  );
}
