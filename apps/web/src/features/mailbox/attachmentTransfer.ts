import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

interface AttachmentActionInput {
  messageId: string;
  attachmentId: string;
}

type AttachmentTransferMetadata = components["schemas"]["AttachmentTransferMetadata"];
type AttachmentChunk = components["schemas"]["AttachmentChunk"];

export interface AttachmentActionResult {
  filename: string;
  opened: boolean;
}

const MAX_ATTACHMENT_CHUNK_BYTES = 256 * 1024;
const INLINE_ATTACHMENT_MIME_TYPES = new Set([
  "text/plain",
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

interface AttachmentTransferDependencies {
  fetchMetadata: (body: {
    message_id: string;
    attachment_id: string;
  }) => Promise<AttachmentTransferMetadata>;
  fetchChunk: (body: {
    message_id: string;
    attachment_id: string;
    offset: number;
    length: number;
  }) => Promise<AttachmentChunk>;
  openWindow: (url: string, target: string) => TransferPopup | null;
}

async function fetchAttachmentBytes(
  input: AttachmentActionInput,
  dependencies: AttachmentTransferDependencies,
) {
  const body = { message_id: input.messageId, attachment_id: input.attachmentId };
  const metadata = await dependencies.fetchMetadata(body);
  if (
    metadata.message_id !== input.messageId ||
    metadata.attachment_id !== input.attachmentId ||
    !isJsonString(metadata.filename) ||
    !isJsonString(metadata.mime_type) ||
    !Number.isSafeInteger(metadata.length_bytes) ||
    metadata.length_bytes < 0 ||
    !Number.isSafeInteger(metadata.max_chunk_bytes) ||
    metadata.max_chunk_bytes < 1 ||
    metadata.max_chunk_bytes > MAX_ATTACHMENT_CHUNK_BYTES ||
    !/^[0-9a-f]{64}$/.test(metadata.sha256)
  ) {
    throw new Error("Attachment transfer metadata was invalid");
  }

  const bytes = new Uint8Array(metadata.length_bytes);
  let offset = 0;
  while (offset < bytes.length) {
    const length = Math.min(metadata.max_chunk_bytes, bytes.length - offset);
    // oxlint-disable-next-line no-await-in-loop -- each validated chunk sets the next byte offset
    const chunk = await dependencies.fetchChunk({ ...body, offset, length });
    if (
      chunk.message_id !== input.messageId ||
      chunk.attachment_id !== input.attachmentId ||
      chunk.offset !== offset ||
      !isJsonString(chunk.data_base64) ||
      chunk.data_base64.length !== Math.ceil(length / 3) * 4
    ) {
      throw new Error("Attachment transfer returned a mismatched chunk");
    }
    let decoded: string;
    try {
      if (
        !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(chunk.data_base64)
      ) {
        throw new Error("invalid base64 encoding");
      }
      decoded = atob(chunk.data_base64);
    } catch {
      throw new Error("Attachment transfer returned invalid base64 data");
    }
    if (decoded.length !== length) throw new Error("Attachment transfer returned a short chunk");
    for (let index = 0; index < decoded.length; index += 1) {
      bytes[offset + index] = decoded.charCodeAt(index);
    }
    offset += length;
  }

  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const sha256 = [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  if (sha256 !== metadata.sha256) throw new Error("Attachment failed its SHA-256 integrity check");
  return { metadata, bytes };
}

function isJsonString(value: string): boolean {
  return Object.prototype.toString.call(value) === "[object String]";
}

function safeDownloadName(filename: string): string {
  const cleaned = Array.from(filename, (character) => {
    const code = character.charCodeAt(0);
    return character === "/" || character === "\\" || code < 0x20 || code === 0x7f
      ? "_"
      : character;
  })
    .join("")
    .trim();
  return cleaned && cleaned !== "." && cleaned !== ".." ? cleaned : "attachment";
}

function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = safeDownloadName(filename);
  link.style.display = "none";
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

interface TransferPopup {
  opener: Window | null;
  closed: boolean;
  close(): void;
  location: Pick<Location, "replace">;
}

const browserDependencies: AttachmentTransferDependencies = {
  fetchMetadata: (body) =>
    apiFetch<AttachmentTransferMetadata>("/api/v1/mail/attachments/transfer/metadata", {
      method: "POST",
      body,
    }),
  fetchChunk: (body) =>
    apiFetch<AttachmentChunk>("/api/v1/mail/attachments/transfer/chunk", {
      method: "POST",
      body,
    }),
  openWindow: (url, target) => window.open(url, target),
};

export async function downloadAttachment(
  input: AttachmentActionInput,
  dependencies: AttachmentTransferDependencies = browserDependencies,
): Promise<AttachmentActionResult> {
  const { metadata, bytes } = await fetchAttachmentBytes(input, dependencies);
  const mimeType = (metadata.mime_type.split(";", 1)[0] ?? "").trim().toLowerCase();
  saveBlob(new Blob([bytes], { type: mimeType }), metadata.filename);
  return { filename: metadata.filename, opened: false };
}

export async function openAttachment(
  input: AttachmentActionInput,
  dependencies: AttachmentTransferDependencies = browserDependencies,
): Promise<AttachmentActionResult> {
  const popup = dependencies.openWindow("about:blank", "_blank");
  if (popup) popup.opener = null;
  try {
    const { metadata, bytes } = await fetchAttachmentBytes(input, dependencies);
    const mimeType = (metadata.mime_type.split(";", 1)[0] ?? "").trim().toLowerCase();
    const blob = new Blob([bytes], { type: mimeType });
    if (!popup || !INLINE_ATTACHMENT_MIME_TYPES.has(mimeType)) {
      if (popup) popup.close();
      saveBlob(blob, metadata.filename);
      return { filename: metadata.filename, opened: false };
    }
    const url = URL.createObjectURL(blob);
    popup.location.replace(url);
    window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
    return { filename: metadata.filename, opened: true };
  } catch (error) {
    if (popup && !popup.closed) popup.close();
    throw error;
  }
}
