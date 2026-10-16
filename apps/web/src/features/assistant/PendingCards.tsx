/*
 * What waits for the user: proofs to sign and questions to answer. The
 * daemon keeps them; approving runs exactly the stored call, not anything
 * this page sends back.
 */

import { Proof } from "./Proof";
import { QuestionCard } from "./Question";
import type { AssistantDecision, AssistantPending } from "./api";

export function PendingCards({
  pending,
  busy,
  onDecide,
}: {
  pending: AssistantPending[];
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  return (
    <div className="space-y-3">
      {pending.map((item) =>
        item.kind === "approval" ? (
          <Proof key={item.call_id} item={item} busy={busy} onDecide={onDecide} />
        ) : (
          <QuestionCard key={item.call_id} item={item} busy={busy} onDecide={onDecide} />
        ),
      )}
    </div>
  );
}
