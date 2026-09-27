/*
 * Generic renderer for daemon diagnostic payloads whose shape varies by
 * version: objects become key/value rows, arrays become ruled lists.
 */

export function DiagnosticValue({ value }: { value: unknown }) {
  if (value === null || value === undefined) {
    return (
      <div className="border-y border-border/60 py-2 text-2xs text-muted-foreground">No data.</div>
    );
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      return (
        <div className="border-y border-border/60 py-2 text-2xs text-muted-foreground">
          No entries.
        </div>
      );
    }
    return (
      <div className="max-h-[360px] overflow-auto border-y border-border/60 py-2">
        <div className="divide-y divide-border/70">
          {value.map((item, index) => (
            <div key={diagnosticKey(item, index)} className="py-1.5 text-2xs">
              {typeof item === "object" && item !== null ? (
                <DiagnosticObject value={item as Record<string, unknown>} compact />
              ) : (
                <span className="font-mono text-muted-foreground">{diagnosticScalar(item)}</span>
              )}
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (typeof value === "object") {
    return (
      <div className="max-h-[360px] overflow-auto border-y border-border/60 py-2">
        <DiagnosticObject value={value as Record<string, unknown>} />
      </div>
    );
  }

  return (
    <div className="border-y border-border/60 py-2 font-mono text-2xs text-muted-foreground">
      {diagnosticScalar(value)}
    </div>
  );
}

function DiagnosticObject({
  value,
  compact,
}: {
  value: Record<string, unknown>;
  compact?: boolean;
}) {
  const entries = Object.entries(value);
  if (entries.length === 0) return <div className="text-2xs text-muted-foreground">Empty.</div>;
  return (
    <dl className={compact ? "grid gap-1" : "grid gap-2"}>
      {entries.map(([key, item]) => (
        <div
          key={key}
          className={
            compact
              ? "grid grid-cols-[120px_1fr] gap-2"
              : "grid grid-cols-[minmax(120px,180px)_1fr] gap-3"
          }
        >
          <dt className="min-w-0 truncate font-mono text-2xs text-muted-foreground">{key}</dt>
          <dd className="min-w-0 break-words font-mono text-2xs text-foreground">
            {diagnosticScalar(item)}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function diagnosticScalar(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  if (Array.isArray(value)) return `${value.length} entries`;
  return JSON.stringify(value);
}

function diagnosticKey(value: unknown, index: number): string {
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    const id = record.id ?? record.event_id ?? record.timestamp ?? record.time;
    if (typeof id === "string" || typeof id === "number") return String(id);
  }
  return String(index);
}
