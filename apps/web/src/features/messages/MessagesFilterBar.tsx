/*
 * The filter line above the people list: one turn choice and two toggles
 * (blueprint 36, "Messages filters are one segmented control and two
 * toggles"). The header counts never change with it. The line says how many
 * rows the filter kept, and Clear filters only shows while a filter is on.
 */

import { Segmented } from "@/components/Segmented";
import { KeyChip } from "@/components/KeyChip";
import { definedShortcut } from "@/lib/actions/definition";
import { cn } from "@/lib/utils";

import {
  cycleMessagesFilter,
  isFiltered,
  MESSAGES_VIEWS,
  NO_FILTER,
  type MessagesFilter,
} from "./filters";

const FILTER_BAR = '[data-testid="messages-filters"]';

/** `g f` lands here, on the segment that is selected. */
export function focusFilterBar(): void {
  document.querySelector<HTMLElement>(`${FILTER_BAR} [role="radio"][tabindex="0"]`)?.focus();
}

export function MessagesFilterBar({
  filter,
  counts,
  onChange,
}: {
  filter: MessagesFilter;
  /**
   * Rows the filter kept (in every band) and every row in Messages. Null while
   * the answer for this filter is still loading, so the line never describes
   * the previous filter.
   */
  counts: { matched: number; total: number } | null;
  onChange: (next: MessagesFilter) => void;
}) {
  const filtered = isFiltered(filter);
  const cycleKey = definedShortcut("messages.cycle-filter");
  return (
    <div
      data-testid="messages-filters"
      className="flex flex-wrap items-center gap-2 px-5 pb-2 pt-3"
    >
      <Segmented
        label="Show"
        className="max-w-full flex-wrap"
        value={filter.turn ?? "attention"}
        options={MESSAGES_VIEWS.map(([id, label]) => [id, label] as const)}
        onChange={(value) => {
          const choice = MESSAGES_VIEWS.find(([id]) => id === value);
          if (choice) onChange({ ...filter, turn: choice[2] });
        }}
      />
      <button
        type="button"
        aria-label="Cycle views"
        aria-keyshortcuts={cycleKey}
        title={`Cycle views (${cycleKey})`}
        onClick={() => onChange(cycleMessagesFilter(filter))}
        className="type-body inline-flex h-7 items-center gap-1.5 px-1 text-text-2 outline-none hover:text-text-1 focus-visible:ring-2 focus-visible:ring-signal"
      >
        <KeyChip>{cycleKey}</KeyChip>
        Cycle views
      </button>
      <Toggle
        label="Unread"
        on={filter.unread}
        onChange={(unread) => onChange({ ...filter, unread })}
      />
      <Toggle
        label="Past your pace"
        on={filter.pastPace}
        onChange={(pastPace) => onChange({ ...filter, pastPace })}
      />
      <p className="type-body flex min-h-7 min-w-0 basis-full items-center justify-end gap-2 text-text-2">
        {filtered ? (
          <>
            {counts ? (
              <>
                <span className="min-w-0 truncate">
                  {counts.matched} shown · {counts.total} need attention
                </span>
                <span aria-hidden="true" className="shrink-0">
                  ·
                </span>
              </>
            ) : null}
            <button
              type="button"
              onClick={() => onChange(NO_FILTER)}
              className="shrink-0 text-text-1 underline underline-offset-2 outline-none focus-visible:ring-2 focus-visible:ring-signal"
            >
              Clear filters
            </button>
          </>
        ) : null}
      </p>
    </div>
  );
}

function Toggle({
  label,
  on,
  onChange,
}: {
  label: string;
  on: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={() => onChange(!on)}
      className={cn(
        "type-body h-7 whitespace-nowrap border px-2.5 outline-none focus-visible:ring-2 focus-visible:ring-signal",
        on
          ? "border-transparent bg-signal-soft text-text-1"
          : "border-rule text-text-2 hover:text-text-1",
      )}
    >
      {label}
    </button>
  );
}
