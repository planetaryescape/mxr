import type { MailboxRow } from "../../shared/types";

/// Format the attachment chip size readout. Mirrors the Rust
/// `format_attachment_chip` ladder: B → K → M with no decimals so the
/// chip stays compact next to the paperclip glyph.
///
/// Returns the empty string when the size is unknown — callers render
/// just the paperclip in that case.
export function formatAttachmentChipSize(sizeBytes: number | null | undefined): string {
  if (sizeBytes === null || sizeBytes === undefined || !Number.isFinite(sizeBytes)) {
    return "";
  }
  const bytes = Math.max(0, Math.trunc(sizeBytes));
  const KIB = 1024;
  const MIB = 1024 * 1024;
  if (bytes >= MIB) {
    return `${Math.trunc(bytes / MIB)}M`;
  }
  if (bytes >= KIB) {
    return `${Math.trunc(bytes / KIB)}K`;
  }
  return `${bytes}B`;
}

/// Smart sender display: prefer a non-empty `sender` (display name);
/// otherwise fall back to the local-part of `sender_detail` (email);
/// otherwise the full email; otherwise a placeholder. Mirrors the
/// TUI's `format_sender` semantics so the two surfaces stay in sync.
export function smartSenderDisplay(row: MailboxRow): string {
  const display = row.sender?.trim();
  if (display) {
    return display;
  }
  const email = row.sender_detail?.trim();
  if (!email) {
    return "(unknown sender)";
  }
  // If we only have an email, prefer the local-part — it reads better
  // in a 22-char column than the full address truncated at the @.
  const at = email.indexOf("@");
  if (at > 0) {
    return email.slice(0, at);
  }
  return email;
}
