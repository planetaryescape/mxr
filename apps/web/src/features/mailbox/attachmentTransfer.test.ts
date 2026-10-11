import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { components } from "@/api/generated";
import { downloadAttachment, openAttachment } from "./api";

type AttachmentTransferMetadata = components["schemas"]["AttachmentTransferMetadata"];
type AttachmentChunk = components["schemas"]["AttachmentChunk"];

const input = { messageId: "message-1", attachmentId: "attachment-1" };
const digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const metadata: AttachmentTransferMetadata = {
  message_id: input.messageId,
  attachment_id: input.attachmentId,
  filename: "note.txt",
  mime_type: "text/plain; charset=utf-8",
  length_bytes: 3,
  sha256: digest,
  max_chunk_bytes: 262144,
};

const fetchMetadata =
  vi.fn<
    (body: { message_id: string; attachment_id: string }) => Promise<AttachmentTransferMetadata>
  >();
const fetchChunk =
  vi.fn<
    (body: {
      message_id: string;
      attachment_id: string;
      offset: number;
      length: number;
    }) => Promise<AttachmentChunk>
  >();

function transferResponses(mimeType = metadata.mime_type, sha256 = digest) {
  fetchMetadata.mockResolvedValueOnce({ ...metadata, mime_type: mimeType, sha256 });
  fetchChunk.mockResolvedValueOnce({
    message_id: input.messageId,
    attachment_id: input.attachmentId,
    offset: 0,
    data_base64: "YWJj",
  });
}

interface TestPopup {
  opener: Window | null;
  closed: boolean;
  close: { (): void; mockReset: () => void };
  location: { replace: { (url: string): void; mockReset: () => void } };
}

let popupOpener: Window | null = window;
let popupClosed = false;
const popup = {
  get opener() {
    return popupOpener;
  },
  set opener(value: Window | null) {
    popupOpener = value;
  },
  get closed() {
    return popupClosed;
  },
  set closed(value: boolean) {
    popupClosed = value;
  },
  close: vi.fn<() => void>(),
  location: { replace: vi.fn<(url: string) => void>() },
} satisfies TestPopup;
let popupResult: typeof popup | null = popup;
const dependencies = { fetchMetadata, fetchChunk, openWindow: () => popupResult };

beforeEach(() => {
  fetchMetadata.mockReset();
  fetchChunk.mockReset();
  popup.opener = window;
  popup.closed = false;
  popupResult = popup;
  popup.close.mockReset();
  popup.location.replace.mockReset();
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
  vi.stubGlobal("URL", {
    createObjectURL: vi.fn<(object: Blob | MediaSource) => string>(() => "blob:attachment"),
    revokeObjectURL: vi.fn<(url: string) => void>(),
  });
});

afterEach(() => vi.restoreAllMocks());

describe("attachment transfer in the browser", () => {
  test("opens verified passive text through a detached popup", async () => {
    transferResponses();

    await expect(openAttachment(input, dependencies)).resolves.toEqual({
      filename: "note.txt",
      opened: true,
    });

    expect(popup.opener).toBeNull();
    expect(popup.location.replace).toHaveBeenCalledWith("blob:attachment");
    expect(fetchChunk).toHaveBeenCalledWith({
      message_id: input.messageId,
      attachment_id: input.attachmentId,
      offset: 0,
      length: 3,
    });
  });

  test("downloads active content and reports that it was not opened", async () => {
    transferResponses("text/html");

    await expect(openAttachment(input, dependencies)).resolves.toEqual({
      filename: "note.txt",
      opened: false,
    });

    expect(popup.close).toHaveBeenCalledOnce();
    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledOnce();
    expect(popup.location.replace).not.toHaveBeenCalled();
  });

  test("downloads verified bytes with the attachment filename", async () => {
    transferResponses();

    await expect(downloadAttachment(input, dependencies)).resolves.toEqual({
      filename: "note.txt",
      opened: false,
    });

    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledOnce();
    expect(URL.createObjectURL).toHaveBeenCalledWith(expect.any(Blob));
  });

  test("rejects corrupted bytes and closes the preparing tab", async () => {
    transferResponses(metadata.mime_type, "0".repeat(64));

    await expect(openAttachment(input, dependencies)).rejects.toThrow("SHA-256 integrity check");

    expect(popup.close).toHaveBeenCalledOnce();
    expect(popup.location.replace).not.toHaveBeenCalled();
    expect(HTMLAnchorElement.prototype.click).not.toHaveBeenCalled();
  });

  test("falls back to download when the browser blocks the popup", async () => {
    popupResult = null;
    transferResponses();

    await expect(openAttachment(input, dependencies)).resolves.toEqual({
      filename: "note.txt",
      opened: false,
    });

    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledOnce();
  });

  test("falls back to download when the user closes the popup during transfer", async () => {
    fetchMetadata.mockResolvedValueOnce(metadata);
    fetchChunk.mockImplementationOnce(async () => {
      popup.closed = true;
      return {
        message_id: input.messageId,
        attachment_id: input.attachmentId,
        offset: 0,
        data_base64: "YWJj",
      };
    });

    await expect(openAttachment(input, dependencies)).resolves.toEqual({
      filename: "note.txt",
      opened: false,
    });

    expect(popup.location.replace).not.toHaveBeenCalled();
    expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledOnce();
  });
});
