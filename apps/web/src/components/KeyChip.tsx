import { cn } from "@/lib/utils";

export function KeyChip({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 items-center rounded border border-border bg-muted px-1.5 font-mono text-2xs text-muted-foreground shadow-sm",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
