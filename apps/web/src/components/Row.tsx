import type { LucideIcon } from "lucide-react";
import { memo, type ComponentPropsWithoutRef, type ReactNode, type Ref } from "react";

import { AccountMark } from "@/components/AccountMark";
import { KeyChip } from "@/components/KeyChip";
import { rowPrimaryTone } from "@/components/rowTone";
import { useRowAccountMark } from "@/features/accounts/useAccountIdentities";
import { cn } from "@/lib/utils";
import { useLanded } from "@/state/landingStore";

export type LampVia = "key" | "pointer";

export interface RowProps extends Omit<ComponentPropsWithoutRef<"li">, "children"> {
  ref?: Ref<HTMLLIElement>;
  lit: boolean;
  via?: LampVia;
  unread?: boolean;
  unreadLabel?: string;
  leaving?: boolean;
  land?: readonly (string | null | undefined)[];
  lead?: ReactNode;
  leadControl?: boolean;
  primary: ReactNode;
  secondary?: ReactNode;
  secondaryDesktopOnly?: boolean;
  trail?: ReactNode;
  time?: ReactNode;
  verbs?: ReactNode;
  renderMain?: (content: ReactNode, className: string) => ReactNode;
  hint?: ReactNode;
  accountId?: string | null;
}

const NO_IDS: readonly string[] = [];

const MAIN = "relative flex min-w-0 flex-1 items-center gap-3 text-left outline-none";

export const Row = memo(function Row({
  lit,
  via = "key",
  unread,
  unreadLabel = "Unread",
  leaving = false,
  land = NO_IDS,
  lead,
  leadControl = false,
  primary,
  secondary,
  secondaryDesktopOnly = false,
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
  const primaryTone = rowPrimaryTone(unread);
  const secondaryTone = unread ? "text-text-1" : "text-text-2";
  const text = (
    <>
      {/* The inbox spells "Unread" into its own aria-label; elsewhere the
          state is typographic, so give assistive tech the same fact without
          changing the visible text. */}
      {unread ? <span className="sr-only">{unreadLabel}. </span> : null}
      {leadControl ? null : lead}
      <span className="min-w-0 flex-1">
        <span className={cn("type-body block truncate", primaryTone)}>{primary}</span>
        {secondary ? (
          <span
            className={cn(
              "type-body block truncate",
              secondaryTone,
              secondaryDesktopOnly && "hidden sm:block",
            )}
          >
            {secondary}
          </span>
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
  Icon?: LucideIcon;
  onClick?: () => void;
  href?: string;
  testId?: string;
  ariaLabel?: string;
  className?: string;
  plain?: boolean;
  title?: string;
  nameFrom?: "sm" | "list";
}) {
  const classes = cn(
    "inline-flex h-7 items-center gap-1.5 whitespace-nowrap px-1.5 hover:text-text-1 focus-visible:text-text-1",
    Icon && "max-sm:min-w-9 max-sm:justify-center",
    className,
  );
  const content = (
    <>
      {keys ? (
        <KeyChip className={cn("h-4 px-1 pointer-coarse:hidden", Icon && "max-sm:hidden")}>
          {keys}
        </KeyChip>
      ) : null}
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
        event.stopPropagation();
        onClick?.();
      }}
      className={classes}
    >
      {content}
    </button>
  );
}
