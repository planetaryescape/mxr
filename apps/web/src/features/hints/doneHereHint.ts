import type { ModeDoneOutcome } from "@/features/modes/modeDone";

import { cachedHint } from "./api";
import { dismissHint } from "./useHint";

const DONE_HERE = "done_here";

type DoneMessage = (done: readonly ModeDoneOutcome[]) => string;

export function withDoneHereHint(message: DoneMessage): DoneMessage {
  const hint = cachedHint(DONE_HERE);
  if (!hint || hint.seen) return message;
  return (done) => {
    dismissHint(DONE_HERE);
    return `${message(done)} ${hint.text}`;
  };
}
