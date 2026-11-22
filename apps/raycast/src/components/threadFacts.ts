import type { Thread, ThreadLabel } from "../bridge/mail";

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kilobytes = Math.round(bytes / 1024);
  if (kilobytes < 1024) return `${kilobytes} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function threadLabels(data: Thread): ThreadLabel[] {
  const seen = new Map<string, ThreadLabel>();
  for (const message of data.messages) {
    for (const label of message.labels ?? []) {
      if (label.kind !== "system") seen.set(label.id, label);
    }
  }
  return [...seen.values()];
}

export function threadAttachments(data: Thread) {
  return data.bodies.flatMap((body) =>
    (body.attachments ?? []).filter((attachment) => !attachment.content_id),
  );
}

export function labelColor(label: ThreadLabel): string | undefined {
  return label.color && /^#[0-9a-f]{6}$/i.test(label.color) ? label.color : undefined;
}
