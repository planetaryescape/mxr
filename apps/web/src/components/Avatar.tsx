import { avatarTone } from "@/components/avatarTone";
import { initials } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * A sender's initials on their stable hue. Inbox, Messages and the thread
 * all draw it here, so one sender wears one colour everywhere. `lg` is the
 * Messages row and the thread card; `md` is a mail row.
 */
export function Avatar({
  name,
  size = "md",
  className,
}: {
  name: string;
  size?: "md" | "lg";
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full font-mono font-semibold",
        size === "lg" ? "size-8 text-xs" : "size-7 text-2xs [[data-density=compact]_&]:size-6",
        avatarTone(name),
        className,
      )}
    >
      {initials(name)}
    </span>
  );
}
