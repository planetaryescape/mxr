import { KeyChip } from "@/components/KeyChip";
import type { ActionScope } from "@/lib/actions/types";

import { useModeGuide, type ModeId } from "./api";

/** The mode whose keys are live, from the scopes its views push. */
export function modeOfScopes(scopes: readonly ActionScope[]): ModeId | null {
  if (scopes.includes("now")) return "now";
  if (scopes.includes("messages")) return "messages";
  if (scopes.includes("archive")) return "archive";
  return scopes.some((scope) => scope === "todo" || scope === "catchup" || scope === "expired")
    ? "todo"
    : null;
}

/**
 * What `?` leads with in a mode: its job, what lands there, how it works
 * and its keys with their verbs. This is the full explanation on demand;
 * the page itself only teaches through hints at their elements.
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
      <h2 className="text-[14px] font-semibold">
        {guide.name}: <span className="font-normal">{guide.header}</span>
      </h2>
      <p className="mt-1 text-[13px] text-muted-foreground">{guide.lands_here}</p>
      <p className="mt-1 max-w-[70ch] text-[13px]">{guide.about}</p>
      <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground">
        {guide.keys.map((key) => (
          <span key={key.key} className="inline-flex items-center gap-1">
            <KeyChip>{key.key}</KeyChip> {key.verb}
          </span>
        ))}
      </p>
    </section>
  );
}
