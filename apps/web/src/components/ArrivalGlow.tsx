/**
 * "Now showing": a one-time glow over the row or header a done just
 * opened, so the move is seen. The parent must be `relative`; key it on
 * the arrival so a second move glows again.
 */
export function ArrivalGlow() {
  return (
    <span
      aria-hidden
      data-testid="arrival"
      className="arrival-glow pointer-events-none absolute inset-0 rounded-[inherit] bg-primary/15"
    />
  );
}
