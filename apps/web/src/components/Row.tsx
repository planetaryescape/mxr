import { memo, type ComponentPropsWithoutRef, type ReactNode, type Ref } from "react";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

/** How the lamp got to a row. Only a pointer fades; a key must feel instant (D101). */
export type LampVia = "key" | "pointer";

export interface RowProps extends Omit<ComponentPropsWithoutRef<"li">, "children"> {
  /** The row's `<li>`, for a page that returns focus to it (React 19 passes `ref` as a prop). */
  ref?: Ref<HTMLLIElement>;
  /** The one lit row: the lamp surface and the leading hairline. */
  lit: boolean;
  via?: LampVia;
  /** Unread: a gutter bar, hidden while lit (the primary line's weight still shows it). */
  unread?: boolean;
  /** The row is folding away (pointer done). Not interactive meanwhile. */
  leaving?: boolean;
  /** Avatar, checkbox, source mark or amount. */
  lead?: ReactNode;
  /** The lead is a control (a checkbox): it sits beside the row's link, not inside it. */
  leadControl?: boolean;
  /** The primary line: Body, `--text-1`, semibold when unread or your turn. */
  primary: ReactNode;
  /** One secondary line: Body, `--text-2`, truncated. */
  secondary?: ReactNode;
  /** Marks and the time. Shown while unlit. */
  trail?: ReactNode;
  /** The row's actions with their keys. Replace the trail on the lit row and on pointer hover. */
  verbs?: ReactNode;
  /**
   * Wrap the lead and text in the row's one interactive element (a link or a
   * button), so the verbs in the trail aren't nested inside it.
   */
  renderMain?: (content: ReactNode, className: string) => ReactNode;
  /** An anchored hint, drawn under the row so it never changes the row's height. */
  hint?: ReactNode;
}

const MAIN = "relative flex min-w-0 flex-1 items-center gap-3 text-left outline-none";

/**
 * One anatomy for every list (D129, docs/blueprint/27 "Rows"): lead, primary
 * line, one secondary line, trail. A mode supplies the content of each slot
 * and cannot add slots. The height is fixed by the density token, so lighting
 * a row, folding a verb in or showing a mark never moves the rows below it.
 *
 * Lit: `--ink-lamp` surface and a 2px `--signal` hairline inside the leading
 * edge. Hovering with a pointer shows the verbs without lighting the row. The
 * verbs take the trail's place on the same line, so only the text's ellipsis
 * can move, never the row's height.
 */
export const Row = memo(function Row({
  lit,
  via = "key",
  unread = false,
  leaving = false,
  lead,
  leadControl = false,
  primary,
  secondary,
  trail,
  verbs,
  renderMain,
  hint,
  className,
  ref,
  ...rest
}: RowProps) {
  const text = (
    <>
      {leadControl ? null : lead}
      <span className="min-w-0 flex-1">
        <span className="type-body block truncate text-text-1">{primary}</span>
        {secondary ? (
          <span className="type-body block truncate text-text-2">{secondary}</span>
        ) : null}
      </span>
    </>
  );
  return (
    <li
      {...rest}
      ref={ref}
      data-lit={lit ? "true" : undefined}
      data-unread={unread ? "true" : undefined}
      data-leaving={leaving ? "true" : undefined}
      className={cn(
        "group/row relative mx-2",
        leaving && "row-fold pointer-events-none",
        className,
      )}
    >
      {/* The unread bar sits in the gutter outside the leading edge, in the text tier, so it never reads as the lamp's cyan hairline. */}
      {unread && !lit ? (
        <span
          aria-hidden
          data-testid="unread-bar"
          className="absolute -left-1.5 inset-y-2 w-[2px] bg-text-1"
        />
      ) : null}
      <div className="relative isolate flex h-(--row-height) items-center gap-3 px-3">
        <span
          aria-hidden
          className={cn(
            "absolute inset-0 -z-10 bg-ink-lamp opacity-0",
            lit && "opacity-100",
            via === "pointer" && "transition-[opacity] duration-fast",
          )}
        />
        <span
          aria-hidden
          className={cn(
            "absolute inset-y-0 left-0 -z-10 w-[2px] bg-signal opacity-0",
            lit && "opacity-100",
            via === "pointer" && "transition-[opacity] duration-fast",
          )}
        />
        {leadControl ? lead : null}
        {renderMain ? renderMain(text, MAIN) : <div className={MAIN}>{text}</div>}
        {trail || verbs ? (
          <div className="type-meta flex shrink-0 items-center text-text-3">
            {trail ? (
              <div
                className={cn(
                  "flex items-center gap-2.5",
                  verbs && (lit ? "hidden" : "pointer-fine:group-hover/row:hidden"),
                )}
              >
                {trail}
              </div>
            ) : null}
            {verbs ? (
              <div
                className={cn(
                  "items-center gap-1 text-text-2",
                  lit ? "flex" : "hidden pointer-fine:group-hover/row:flex",
                )}
              >
                {verbs}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
      {hint}
    </li>
  );
});

/** One verb on a lit row: its key and its name, as in "r reply". */
export function RowVerb({
  keys,
  label,
  onClick,
  testId,
  ariaLabel,
  className,
}: {
  keys: string;
  label: string;
  onClick: () => void;
  testId?: string;
  ariaLabel?: string;
  className?: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      aria-label={ariaLabel ?? label}
      onClick={(event) => {
        // The row under it is also clickable; a verb is its own act.
        event.stopPropagation();
        onClick();
      }}
      className={cn(
        "inline-flex h-7 items-center gap-1.5 whitespace-nowrap px-1.5 hover:text-text-1 focus-visible:text-text-1",
        className,
      )}
    >
      <KeyChip className="h-4 px-1 pointer-coarse:hidden">{keys}</KeyChip>
      <span className="hidden sm:inline">{label}</span>
      <span className="sm:hidden">{label.split(" ")[0]}</span>
    </button>
  );
}
