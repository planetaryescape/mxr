interface SendIdentity {
  intentKey: string;
  sendId: string;
}

export type SendEvent =
  | ({ kind: "queued" } & SendIdentity)
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
