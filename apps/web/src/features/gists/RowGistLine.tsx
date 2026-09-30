import type { RowGist } from "./rowGists";

/** The line in words, for a row's accessible name. */
export function gistText(gist: RowGist): string {
  return gist.ask ? `Asks you to ${gist.ask}. ${gist.about}` : gist.about;
}

/**
 * Hover text: the line in full, then where it came from. The ask is the
 * model's own phrasing, and says so; the reader highlights the verified
 * sentence itself.
 */
export function gistTitle(gist: RowGist): string {
  return `${gistText(gist)}\n${gist.source}, summarised by the model`;
}

/** "Asks: confirm who owns the rollout check", in the reader's ask colour. */
export function GistAsk({ ask }: { ask: string }) {
  return (
    <>
      <span className="font-medium text-warning">Asks:</span>{" "}
      <span className="text-foreground/85">{ask}</span>
    </>
  );
}

/**
 * The inbox row's snippet line with a gist: the ask first (it is what
 * triage decides on), then what the conversation is about.
 */
export function RowGistLine({ gist }: { gist: RowGist }) {
  return (
    <span data-testid="row-gist" title={gistTitle(gist)}>
      {gist.ask ? (
        <>
          <GistAsk ask={gist.ask} />
          <span aria-hidden> · </span>
        </>
      ) : null}
      {gist.about}
    </span>
  );
}
