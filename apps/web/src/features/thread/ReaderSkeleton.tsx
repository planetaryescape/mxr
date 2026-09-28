/* Placeholder reader while a conversation loads. */

/** `quiet`: the same frame, empty, for the first 300 ms of a load. */
export function ReaderSkeleton({ quiet = false }: { quiet?: boolean }) {
  if (quiet) {
    return (
      <div
        className="flex min-w-0 flex-1 flex-col"
        aria-busy="true"
        aria-label="Loading conversation"
      />
    );
  }
  return (
    <div
      className="flex min-w-0 flex-1 flex-col"
      aria-busy="true"
      aria-label="Loading conversation"
      data-testid="reader-skeleton"
    >
      <div className="border-b border-border px-5 py-4">
        <div className="h-5 w-2/3 animate-pulse rounded bg-muted" />
        <div className="mt-2 h-3 w-1/3 animate-pulse rounded bg-muted/70" />
      </div>
      {[0, 1].map((index) => (
        <div key={index} className="border-b border-border/70 px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="size-8 animate-pulse rounded-full bg-muted" />
            <div className="h-3 w-40 animate-pulse rounded bg-muted" />
          </div>
          <div className="ml-11 mt-4 grid gap-2">
            <div className="h-3 w-5/6 animate-pulse rounded bg-muted/70" />
            <div className="h-3 w-4/6 animate-pulse rounded bg-muted/70" />
            <div className="h-3 w-3/6 animate-pulse rounded bg-muted/70" />
          </div>
        </div>
      ))}
    </div>
  );
}
