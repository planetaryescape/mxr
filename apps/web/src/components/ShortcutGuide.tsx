import { useEffect, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { getRegistry, useActionContext } from "@/lib/actions";
import { formatChord, isMacPlatform } from "@/lib/keys/chord";
import { shortcutContinuations } from "@/lib/keys/continuations";
import { useKeyScope } from "@/state/keyScopeStore";

/** A pause after a prefix teaches its next keys without opening a modal. */
export function ShortcutGuide() {
  const prefix = useKeyScope((s) => s.pendingPrefix);
  const context = useActionContext();
  const [shown, setShown] = useState<string | null>(null);
  useEffect(() => {
    setShown(null);
    if (!prefix) return;
    const timer = setTimeout(() => setShown(prefix), 200);
    return () => clearTimeout(timer);
  }, [prefix]);
  if (!prefix || shown !== prefix) return null;
  const choices = shortcutContinuations(getRegistry(), prefix, context, isMacPlatform());
  if (choices.length === 0) return null;
  return (
    <aside
      aria-label="Next shortcut keys"
      className="fixed bottom-16 right-4 z-40 max-h-[65vh] w-[min(44rem,calc(100vw-2rem))] overflow-y-auto border border-rule bg-background p-4 shadow-lg"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <p className="type-body text-text-1">
          <KeyChip className="pointer-coarse:inline-flex">{formatChord(prefix)}</KeyChip> then…
        </p>
        <p className="type-meta text-text-2">
          <KeyChip className="pointer-coarse:inline-flex">Esc</KeyChip> cancel
        </p>
      </div>
      <ul className="grid grid-cols-2 gap-x-3 gap-y-2 sm:gap-x-5">
        {choices.slice(0, 20).map((choice) => (
          <li key={choice.chord} className="type-body flex items-start gap-2 text-text-1">
            <KeyChip className="shrink-0 pointer-coarse:inline-flex">
              {formatChord(choice.next)}
            </KeyChip>
            <span>{choice.label}</span>
          </li>
        ))}
      </ul>
      {choices.length > 20 ? (
        <p className="type-meta mt-3 text-text-2">
          20 of {choices.length} shortcuts · Esc, then ? shows all keys.
        </p>
      ) : null}
    </aside>
  );
}
