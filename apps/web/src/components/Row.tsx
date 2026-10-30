import type { LucideIcon } from "lucide-react";
import { memo, type ComponentPropsWithoutRef, type ReactNode, type Ref } from "react";

import { AccountMark } from "@/components/AccountMark";
import { KeyChip } from "@/components/KeyChip";
import { useRowAccountMark } from "@/features/accounts/useAccountIdentities";
import { cn } from "@/lib/utils";
import { useLanded } from "@/state/landingStore";

/** How the lamp got to a row. Only a pointer fades; a key must feel instant (D101). */
export type LampVia = "key" | "pointer";

export interface RowProps extends Omit<ComponentPropsWithoutRef<"li">, "children"> {
  /** The row's `<li>`, for a page that returns focus to it (React 19 passes `ref` as a prop). */
  ref?: Ref<HTMLLIElement>;
  /** The one lit row: the lamp surface, and nothing on the leading edge. */
  lit: boolean;
  via?: LampVia;
  /**
   * Unread mail reads bright and semibold; read mail calms to `--text-2`.
   * Omit it where the list has no read state (To do, Updates, drafts), and the
   * row keeps the neutral text tier.
   */
  unread?: boolean;
  /** The row is folding away (pointer done). Not interactive meanwhile. */
  leaving?: boolean;
  /**
   * Message and thread ids this row stands for. When a change the assistant
   * or a sync made lands on any of them, the row wears the landing underline
   * for 600ms (docs/blueprint/26 "Motion").
   */
  land?: readonly (string | null | undefined)[];
  /** Avatar, checkbox, source mark or amount. */
  lead?: ReactNode;
  /** The lead is a control (a checkbox): it sits beside the row's link, not inside it. */
  leadControl?: boolean;
  /** The primary line: Body, semibold and `--text-1` when unread, `--text-2` when read. */
  primary: ReactNode;
  /** One secondary line: Body, `--text-1` while unread, else `--text-2`, truncated. */
  secondary?: ReactNode;
  /** Marks and values. Shown while unlit; the lit row shows the verbs in their place. */
  trail?: ReactNode;
  /**
   * The row's time. Shown lit or not, after the trail or the verbs, so the row
   * you are on keeps when it arrived (D148, docs/blueprint/27 "Trail").
   */
  time?: ReactNode;
  /** The row's actions with their keys. Replace the trail on the lit row and on pointer hover. */
  verbs?: ReactNode;
  /**
   * Wrap the lead and text in the row's one interactive element (a link or a
   * button), so the verbs in the trail aren't nested inside it.
   */
  renderMain?: (content: ReactNode, className: string) => ReactNode;
  /** An anchored hint, drawn under the row so it never changes the row's height. */
  hint?: ReactNode;
  /**
   * The account the row's mail came to. Drawn as the account's chip, between
   * the trail and the time, when the view mixes accounts (docs/blueprint/33).
   */
  accountId?: string | null;
}

const NO_IDS: readonly string[] = [];

const MAIN = "relative flex min-w-0 flex-1 items-center gap-3 text-left outline-none";

/**
 * One anatomy for every list (D129, docs/blueprint/27 "Rows"): lead, primary
 * line, one secondary line, trail. A mode supplies the content of each slot
 * and cannot add slots. The height is fixed by the density token, so lighting
 * a row, folding a verb in or showing a mark never moves the rows below it.
 *
 * Lit: `--ink-lamp` surface, and nothing on the leading edge. Unread is the
 * text's job (semibold and `--text-1` against a read row's `--text-2`), so the
 * left edge says only where you are. Hovering with a pointer shows the verbs
 * without lighting the row. The verbs take the trail's place on the same line,
 * so only the text's ellipsis can move, never the row's height.
 */
export const Row = memo(function Row({
  lit,
  via = "key",
  unread,
  leaving = false,
  land = NO_IDS,
  lead,
  leadControl = false,
  primary,
  secondary,
  trail,
  time,
  verbs,
  renderMain,
  hint,
  accountId,
  className,
  ref,
  ...rest
}: RowProps) {
  const landed = useLanded(...land);
  const identity = useRowAccountMark(accountId);
  // `undefined` means the list has no read state, so the row keeps the neutral
  // tier. Only an explicit `false` calms the row.
  const read = unread === false;
  const primaryTone = unread ? "font-semibold text-text-1" : read ? "text-text-2" : "text-text-1";
  const secondaryTone = unread ? "text-text-1" : "text-text-2";
  const text = (
    <>
      {/* The inbox spells "Unread" into its own aria-label; elsewhere the
          state is typographic, so give assistive tech the same fact without
          changing the visible text. */}
      {unread ? <span className="sr-only">Unread. </span> : null}
      {leadControl ? null : lead}
      <span className="min-w-0 flex-1">
        <span className={cn("type-body block truncate", primaryTone)}>{primary}</span>
        {secondary ? (
          <span className={cn("type-body block truncate", secondaryTone)}>{secondary}</span>
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
        landed && "landed",
        className,
      )}
    >
      {/* The lamp surface is the one mark of where you are: no leading-edge
          hairline, so nothing sits at a different inset from anything else. */}
      <div className="relative isolate flex h-(--row-height) items-center gap-3 px-3">
        <span
          aria-hidden
          className={cn(
            "absolute inset-0 -z-10 bg-ink-lamp opacity-0",
            lit && "opacity-100",
            via === "pointer" && "transition-[opacity] duration-fast",
          )}
        />
        {leadControl ? lead : null}
        {renderMain ? renderMain(text, MAIN) : <div className={MAIN}>{text}</div>}
        {trail || verbs || time || identity ? (
          // Below sm the group takes at most 60% of the row, so the primary
          // text keeps room. Inside it the actions and marks clip first; the
          // time never does.
          <div className="type-meta flex max-sm:max-w-[60%] shrink-0 items-center text-text-3">
            {trail ? (
              <div
                className={cn(
                  "flex min-w-0 items-center gap-2.5 max-sm:overflow-hidden",
                  verbs && (lit ? "hidden" : "pointer-fine:group-hover/row:hidden"),
                )}
              >
                {trail}
              </div>
            ) : null}
            {identity ? <AccountMark identity={identity} className="ml-2.5" /> : null}
            {verbs ? (
              <div
                className={cn(
                  "min-w-0 items-center gap-1 text-text-2 max-sm:overflow-hidden",
                  lit ? "flex" : "hidden pointer-fine:group-hover/row:flex",
                )}
              >
                {verbs}
              </div>
            ) : null}
            {time ? (
              <div
                className={cn(
                  "flex shrink-0 items-center whitespace-nowrap tabular-nums",
                  identity ? "ml-1.5" : "ml-2.5",
                )}
              >
                {time}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
      {hint}
    </li>
  );
});

/**
 * One verb on a lit row: its key and its name, as in "r reply". A verb with
 * no key (a tool page's action) shows its name only; with `href` it is a link
 * that leaves the app, drawn the same. `plain` draws it as text for a row
 * that is itself a listbox option, which can't hold controls: the keyboard
 * runs the verb with its key, the pointer with a click on the text.
 */
export function RowVerb({
  keys,
  label,
  onClick,
  href,
  testId,
  ariaLabel,
  className,
  Icon,
  plain = false,
  title,
  nameFrom = "sm",
}: {
  keys?: string;
  label: string;
  /** On a phone the verb is this icon alone, so four of them still leave the row its name. */
  Icon?: LucideIcon;
  onClick?: () => void;
  href?: string;
  testId?: string;
  ariaLabel?: string;
  className?: string;
  plain?: boolean;
  title?: string;
  /**
   * Where the name shows: from the viewport's `sm` width, or only once the
   * list is wide (`@xl/list`), so a narrow list with a reader beside it keeps
   * its room for the sender and the keys alone say it.
   */
  nameFrom?: "sm" | "list";
}) {
  const classes = cn(
    "inline-flex h-7 items-center gap-1.5 whitespace-nowrap px-1.5 hover:text-text-1 focus-visible:text-text-1",
    Icon && "max-sm:min-w-9 max-sm:justify-center",
    className,
  );
  const content = (
    <>
      {keys ? <KeyChip className="h-4 px-1 pointer-coarse:hidden">{keys}</KeyChip> : null}
      {Icon ? <Icon aria-hidden className="size-4 sm:hidden" /> : null}
      <span className={nameFrom === "list" ? "hidden @xl/list:inline" : "hidden sm:inline"}>
        {label}
      </span>
      {Icon || nameFrom === "list" ? null : (
        <span className="sm:hidden">{label.split(" ")[0]}</span>
      )}
    </>
  );
  if (plain) {
    return (
      <span
        aria-hidden
        title={title ?? `${label} (${keys})`}
        onClick={(event) => {
          event.stopPropagation();
          onClick?.();
        }}
        className={cn(classes, "cursor-pointer")}
      >
        {content}
      </span>
    );
  }
  if (href) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noreferrer noopener"
        data-testid={testId}
        aria-label={ariaLabel ?? label}
        onClick={(event) => event.stopPropagation()}
        className={classes}
      >
        {content}
      </a>
    );
  }
  return (
    <button
      type="button"
      data-testid={testId}
      aria-label={ariaLabel ?? label}
      onClick={(event) => {
        // The row under it is also clickable; a verb is its own act.
        event.stopPropagation();
        onClick?.();
      }}
      className={classes}
    >
      {content}
    </button>
  );
}
