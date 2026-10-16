/*
 * An empty conversation: what the desk is for, and three things to start
 * with, the current page's first. No illustration, no robot.
 */

import { useRouterState } from "@tanstack/react-router";

import { Row } from "@/components/Row";

/** The suggestions for the page the person is on, then the general ones. */
export function suggestionsFor(view: string): string[] {
  const here: Record<string, string> = {
    now: "What needs me today?",
    messages: "Summarise who's waiting on me in Messages",
    todo: "What is due this week, and what can wait?",
    updates: "Summarise today's updates",
    reading: "What is worth reading first on my Reading list?",
    archive: "Find the emails I'd most regret losing",
  };
  const general = [
    "Who is waiting on me?",
    "Find newsletters I never open and tidy them up",
    "Search for CI failure emails and archive them",
  ];
  const first = here[view];
  return (first ? [first, ...general] : general).slice(0, 3);
}

export function EmptyState({
  disabled,
  onPick,
}: {
  disabled: boolean;
  onPick: (text: string) => void;
}) {
  const view = useRouterState({
    select: (state) => state.location.pathname.split("/").find(Boolean) ?? "now",
  });
  return (
    <div className="flex min-h-full flex-col justify-end gap-4 pb-2">
      <h3 className="type-heading px-1 text-text-1">
        Ask about your mail, or have it do the tedious part.
      </h3>
      <ul aria-label="Ways to start" className="-mx-3">
        {suggestionsFor(view).map((text) => (
          <Row
            key={text}
            lit={false}
            className="mx-0"
            primary={<span className="text-text-2">{text}</span>}
            renderMain={(content, className) => (
              <button
                type="button"
                disabled={disabled}
                onClick={() => onPick(text)}
                className={`${className} disabled:opacity-50 focus-visible:text-text-1`}
              >
                {content}
              </button>
            )}
          />
        ))}
      </ul>
    </div>
  );
}
