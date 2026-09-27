/*
 * What happens to a send after Send is pressed, for views that outlive the
 * composer. A send finishes after its undo window, often once the composer
 * that started it has closed (focus mode moves on to the next reply), so
 * the composer can't tell anyone itself. A plain listener set: there is one
 * producer (useComposeSend) and a handful of subscribers.
 */

interface SendIdentity {
  /** The compose intent that sent it. */
  intentKey: string;
  /** This one dispatched send; a resend of the same intent gets a new id. */
  sendId: string;
}

export type SendEvent =
  /** The undo window opened (or, with no window, the send started). */
  | ({ kind: "queued" } & SendIdentity)
  /** Undo was pressed inside the window: nothing went out. */
  | ({ kind: "cancelled" } & SendIdentity)
  | ({ kind: "sent"; sentMessageId?: string } & SendIdentity)
  | ({ kind: "failed" } & SendIdentity);

type Listener = (event: SendEvent) => void;

const listeners = new Set<Listener>();

export function onSendEvent(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function emitSendEvent(event: SendEvent): void {
  for (const listener of listeners) listener(event);
}
