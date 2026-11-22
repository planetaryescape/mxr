import { modelMarkTitle } from "@/components/ModelMark";

import type { RowGist } from "./rowGists";

export function gistText(gist: RowGist): string {
  return gist.ask ? `Asks you to ${gist.ask}. ${gist.about}` : gist.about;
}

export function gistTitle(gist: RowGist): string {
  const { provenance } = gist;
  const mark = provenance
    ? modelMarkTitle(provenance.model, provenance.locality === "cloud")
    : "Written by a model";
  return `${gistText(gist)}\n${gist.source}. ${mark}`;
}

export function GistAsk({ ask }: { ask: string }) {
  return (
    <>
      <span className="font-medium text-warning">Asks:</span>{" "}
      <span className="text-foreground/85">{ask}</span>
    </>
  );
}

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
