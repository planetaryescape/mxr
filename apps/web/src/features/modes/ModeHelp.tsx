import type { ActionScope } from "@/lib/actions/types";

import { useModeGuide, type ModeId } from "./api";

/** The mode whose keys are live, from the scopes its views push. */
export function modeOfScopes(scopes: readonly ActionScope[]): ModeId | null {
  if (scopes.includes("now")) return "now";
  if (scopes.includes("messages")) return "messages";
  if (scopes.includes("archive")) return "archive";
  if (scopes.includes("updates")) return "updates";
  if (scopes.includes("reading") || scopes.includes("reading-reader")) return "reading";
  return scopes.some((scope) => scope === "todo" || scope === "catchup" || scope === "expired")
    ? "todo"
    : null;
}

/**
 * What `?` leads with in a mode: its purpose, once, and how it works. The
 * keys follow in the sheet's own list, so they aren't repeated here.
 */
export function ModeHelp({ mode }: { mode: ModeId }) {
  const guide = useModeGuide(mode).data;
  if (!guide) return null;
  return (
    <section
      aria-label={`About ${guide.name}`}
      data-testid="mode-help"
      className="border-l-2 border-primary pl-3"
    >
      <h2 className="type-heading">
        {guide.name}: <span className="font-normal text-text-2">{guide.header}</span>
      </h2>
      <p className="mt-1 max-w-[70ch] text-sm text-text-2">{guide.about}</p>
    </section>
  );
}
