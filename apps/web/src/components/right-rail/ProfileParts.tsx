/* Stat tile and label/value row used by the sender and relationship panels. */

export function ProfileStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border bg-muted/30 px-3 py-2">
      <div className="font-mono text-base font-semibold">{value}</div>
      <div className="mt-0.5 text-2xs uppercase tracking-wide text-muted-foreground">{label}</div>
    </div>
  );
}

export function ProfileRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[92px_1fr] gap-2 text-xs">
      <div className="text-muted-foreground">{label}</div>
      <div className="min-w-0 break-words text-right font-medium">{value}</div>
    </div>
  );
}
