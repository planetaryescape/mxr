import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { SetStateAction } from "react";
import type { ComposeDraftState } from "./composeDraft";
import { useComposeAttachments } from "./useComposeAttachments";

const api = vi.hoisted(() => ({
  upload: vi.fn<(input: { operation: string; [key: string]: unknown }) => Promise<unknown>>(),
  supports: vi.fn<() => Promise<boolean>>(),
}));
vi.mock("../api", () => ({
  uploadComposeAttachment: api.upload,
  supportsDraftAttachmentUpload: api.supports,
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn<() => void>(), success: vi.fn<() => void>() } }));

function fixture(): ComposeDraftState {
  return {
    draftId: "draft-1",
    revision: 1,
    draftPath: "/compose/one.md",
    rawContent: "",
    bodyMarkdown: "Please read the attachment.",
    frontmatter: {
      to: "alice@example.com",
      cc: "",
      bcc: "",
      from: "me@example.com",
      subject: "Notes",
      attach: [],
    },
    issues: [],
    accountId: "account-1",
    kind: "new",
  };
}
function setup() {
  const draftRef: { current: ComposeDraftState | null } = { current: fixture() };
  const setDraft = vi.fn<(value: SetStateAction<ComposeDraftState | null>) => void>((value) => {
    draftRef.current = typeof value === "function" ? value(draftRef.current) : value;
  });
  const setDirty = vi.fn<(value: SetStateAction<boolean>) => void>();
  const hook = renderHook(() => useComposeAttachments({ draftRef, setDraft, setDirty }));
  return { ...hook, draftRef, setDraft, setDirty };
}
function clientFile(length: number) {
  const bytes = new Uint8Array(length);
  const file = new File([bytes], "notes.bin", { type: "application/octet-stream" });
  const read = vi.fn<() => Promise<ArrayBuffer>>(async () => bytes.buffer);
  Object.defineProperty(file, "arrayBuffer", { value: read });
  return { file, read };
}

beforeEach(() => {
  api.upload.mockReset();
  api.supports.mockReset();
  api.supports.mockResolvedValue(true);
  vi.stubGlobal("crypto", {
    randomUUID: () => "resource-1",
    subtle: { digest: async () => new Uint8Array(32).buffer },
  });
  api.upload.mockImplementation(async (input) =>
    input.operation === "begin"
      ? { draft_id: "draft-1", revision: 1 }
      : input.operation === "chunk"
        ? { ok: true }
        : {
            draft_id: "draft-1",
            revision: 2,
            path: "mxr-resource:resource-1/notes.bin",
            filename: "notes.bin",
            size_bytes: 3,
          },
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
});

test("upload uses bounded chunks and the finalized revision for the next edit", async () => {
  const state = setup();
  const input = clientFile(256 * 1024 + 3);
  await act(async () => {
    await state.result.current.addFiles([input.file]);
  });
  const chunks = api.upload.mock.calls
    .map(([request]) => request)
    .filter((request) => request.operation === "chunk");
  expect(chunks).toHaveLength(2);
  expect(chunks[0]).toMatchObject({ offset: 0 });
  expect(chunks[1]).toMatchObject({ offset: 256 * 1024 });
  expect(state.draftRef.current?.revision).toBe(2);
  expect(state.draftRef.current?.frontmatter.attach).toEqual(["mxr-resource:resource-1/notes.bin"]);
  expect(state.setDirty).toHaveBeenCalledWith(true);
  state.setDraft((draft) => (draft ? { ...draft, bodyMarkdown: "Edited after upload." } : draft));
  expect(state.draftRef.current?.revision).toBe(2);
});

test("an upload completed after switching drafts cannot attach to the new draft", async () => {
  const state = setup();
  const input = clientFile(3);
  api.upload.mockImplementation(async (request) => {
    if (request.operation === "finalize") {
      state.draftRef.current = { ...fixture(), draftId: "draft-2", draftPath: "/compose/two.md" };
      return { draft_id: "draft-1", revision: 2, path: "mxr-resource:resource-1/notes.bin" };
    }
    return request.operation === "begin" ? { draft_id: "draft-1", revision: 1 } : { ok: true };
  });
  await act(async () => {
    await state.result.current.addFiles([input.file]);
  });
  expect(state.draftRef.current?.draftId).toBe("draft-2");
  expect(state.draftRef.current?.frontmatter.attach).toEqual([]);
  expect(state.setDraft).not.toHaveBeenCalled();
  expect(state.setDirty).not.toHaveBeenCalled();
});

test("an older daemon rejects upload before reading client bytes", async () => {
  const state = setup();
  const input = clientFile(3);
  api.supports.mockResolvedValue(false);
  await act(async () => {
    await state.result.current.addFiles([input.file]);
  });
  expect(input.read).not.toHaveBeenCalled();
  expect(api.upload).not.toHaveBeenCalled();
});
