import { describe, expect, it } from "vitest";
import type { MailboxRow } from "../../shared/types";
import { formatAttachmentChipSize, smartSenderDisplay } from "./MailRow.formatters";

function row(overrides: Partial<MailboxRow> = {}): MailboxRow {
  return {
    id: "msg-1",
    thread_id: "t-1",
    provider_id: "gmail-1",
    sender: "Alice",
    sender_detail: "alice@example.com",
    subject: "Subject",
    snippet: "Snippet",
    date_label: "2h",
    unread: false,
    starred: false,
    has_attachments: false,
    ...overrides,
  };
}

describe("formatAttachmentChipSize", () => {
  it("returns empty string when size is missing", () => {
    expect(formatAttachmentChipSize(null)).toBe("");
    expect(formatAttachmentChipSize(undefined)).toBe("");
  });

  it("formats sub-kilobyte values in bytes", () => {
    expect(formatAttachmentChipSize(0)).toBe("0B");
    expect(formatAttachmentChipSize(512)).toBe("512B");
  });

  it("formats kilobyte values without decimals", () => {
    // 45 * 1024 = 46080
    expect(formatAttachmentChipSize(46080)).toBe("45K");
  });

  it("formats megabyte values without decimals", () => {
    // 2.5 MiB
    expect(formatAttachmentChipSize(2.5 * 1024 * 1024)).toBe("2M");
  });

  it("clamps non-finite sizes to empty", () => {
    expect(formatAttachmentChipSize(Number.NaN)).toBe("");
    expect(formatAttachmentChipSize(Number.POSITIVE_INFINITY)).toBe("");
  });
});

describe("smartSenderDisplay", () => {
  it("prefers display name when present", () => {
    expect(smartSenderDisplay(row({ sender: "Alice Smith" }))).toBe("Alice Smith");
  });

  it("falls back to email local-part when display is blank", () => {
    expect(smartSenderDisplay(row({ sender: "   ", sender_detail: "bob@example.com" }))).toBe(
      "bob",
    );
  });

  it("returns the full email when no @ is present", () => {
    expect(smartSenderDisplay(row({ sender: "", sender_detail: "weird-handle" }))).toBe(
      "weird-handle",
    );
  });

  it("returns a placeholder when both fields are empty", () => {
    expect(smartSenderDisplay(row({ sender: "", sender_detail: null }))).toBe("(unknown sender)");
  });
});
