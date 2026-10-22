/*
 * The filter line above the people list: one turn choice and two toggles
 * (blueprint 36, "Messages filters are one segmented control and two
 * toggles"). The header counts never change with it. The line says how many
 * rows the filter kept, and Clear filters only shows while a filter is on.
 */

import { Segmented } from "@/components/Segmented";
import { cn } from "@/lib/utils";

import type { MessagesTurn } from "./api";
import { isFiltered, NO_FILTER, type MessagesFilter } from "./filters";

const FILTER_BAR = '[data-testid="messages-filters"]';

/** `g f` lands here, on the segment that is selected. */
export function focusFilterBar(): void {
  document.querySelector<HTMLElement>(`${FILTER_BAR} [role="radio"][tabindex="0"]`)?.focus();
}

const TURN_OPTIONS: readonly (readonly [string, string])[] = [
  ["all", "All"],
  ["mine", "Your turn"],
  ["theirs", "Waiting on them"],
];

export function MessagesFilterBar({
  filter,
  matched,
  total,
  onChange,
}: {
  filter: MessagesFilter;
  /** Rows the filter kept, in every band. */
  matched: number;
  /** Every row in Messages, unfiltered. */
  total: number;
  onChange: (next: MessagesFilter) => void;
}) {
  const filtered = isFiltered(filter);
  return (
    <div
      data-testid="messages-filters"
      className="flex flex-wrap items-center gap-2 px-5 pb-2 pt-3"
    >
      <Segmented
        label="Show"
        value={filter.turn ?? "all"}
        options={TURN_OPTIONS}
        onChange={(value) =>
          onChange({ ...filter, turn: value === "all" ? null : (value as MessagesTurn) })
        }
      />
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
      {filtered ? (
        <p className="type-body ml-auto flex items-center gap-2 text-text-2">
          <span>
            Showing {matched} of {total}
          </span>
          <span aria-hidden="true">·</span>
          <button
            type="button"
            onClick={() => onChange(NO_FILTER)}
            className="text-text-1 underline underline-offset-2 outline-none focus-visible:ring-2 focus-visible:ring-signal"
          >
            Clear filters
          </button>
        </p>
      ) : null}
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
