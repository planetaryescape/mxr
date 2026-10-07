import type { ModeDoneOutcome } from "@/features/modes/modeDone";

import { cachedHint } from "./api";
import { dismissHint } from "./useHint";

const DONE_HERE = "done_here";

type DoneMessage = (done: readonly ModeDoneOutcome[]) => string;

/**
 * The first done here on a conversation: the handoff toast carries the
 * hint, since the toast is already where the user looks to see where the
 * item went. Once seen, `message` comes back as it was.
 */
export function withDoneHereHint(message: DoneMessage): DoneMessage {
  const hint = cachedHint(DONE_HERE);
  if (!hint || hint.seen) return message;
  return (done) => {
    // Seen once it is on screen, which only happens when the done worked.
    dismissHint(DONE_HERE);
    return `${message(done)} ${hint.text}`;
  };
}
