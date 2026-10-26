/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useRef, useState, type ReactNode } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { toast } from "sonner";
import { observeWorkspace, resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { archiveMessages } from "@/features/mailbox/api";
import { draftFromSession, type ComposeDraftState } from "./composeDraft";
import { useComposeSend } from "./useComposeSend";
import { sendComposeSession } from "../api";
import { emitSendEvent } from "./sendEvents";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  checkComposeSafety: vi
    .fn<typeof import("../api").checkComposeSafety>()
    .mockResolvedValue({ report: { allowed: true, verdict: "safe", issues: [] } }),
  sendComposeSession: vi.fn<typeof import("../api").sendComposeSession>(),
}));
vi.mock("@/features/drafts/sendOutcomes", () => ({
  prepareReviewedSend: vi.fn<typeof import("@/features/drafts/sendOutcomes").prepareReviewedSend>(
    async (reference: import("@/features/drafts/sendOutcomes").SendReference) => reference,
  ),
  retainSendReference: vi.fn<typeof import("@/features/drafts/sendOutcomes").retainSendReference>(),
}));
vi.mock("@/features/promises/api", () => ({
  detectComposePromises: vi
    .fn<typeof import("@/features/promises/api").detectComposePromises>()
    .mockRejectedValue(new Error("no model needed")),
}));
vi.mock("@/features/mailbox/api", () => ({
  archiveMessages: vi.fn<typeof import("@/features/mailbox/api").archiveMessages>(),
}));
vi.mock("./sendEvents", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./sendEvents")>()),
  emitSendEvent: vi.fn<typeof import("./sendEvents").emitSendEvent>(),
}));
vi.mock("sonner", () => ({
  toast: {
    info: vi.fn<typeof import("sonner").toast.info>(),
    success: vi.fn<typeof import("sonner").toast.success>(),
    error: vi.fn<typeof import("sonner").toast.error>(),
  },
}));

beforeEach(() => {
  resetWorkspaceOwnerForTest();
  useUiPrefs.setState({ undoSendSeconds: 0 });
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("a deferred approved send shows waiting without claiming sent, archiving, or closing", async () => {
  const binding = {
    target: window.location.origin,
    workspace: { workspace_id: "send-owner", recovery_generation: "send-generation" },
  };
  observeWorkspace(binding);
  const initial = draftFromSession({
    binding,
    draftPath: "/isolated/waiting.md",
    draftId: "waiting-draft",
    revision: 2,
    accountId: "synthetic-account",
    rawContent: "",
    bodyMarkdown: "Approved reply",
    previewHtml: "",
    issues: [],
    frontmatter: {
      from: "me@example.com",
      to: "synthetic@example.com",
      cc: "",
      bcc: "",
      subject: "Approved",
      attach: [],
    },
  });
  const retryAt = "2026-10-26T12:15:00Z";
  vi.mocked(sendComposeSession).mockResolvedValue({
    ok: true,
    state: "waiting",
    operation_id: "waiting-draft:2",
    retry_at: retryAt,
  });
  const finished = vi.fn<() => void>();
  const onSent = vi.fn<() => void>();
  const navigate = vi.fn<() => Promise<void>>(async () => undefined);
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const hook = renderHook(
    () => {
      const [draft, setDraft] = useState<ComposeDraftState | null>(initial);
      const draftRef = useRef(draft);
      return useComposeSend({
        intent: { key: "reply:source", kind: "reply", title: "Reply", messageId: "source" },
        options: { onSent },
        navigate,
        queryClient: client,
        draftRef,
        setDraft,
        saveCurrentDraft: async () => undefined,
        isCurrentDraftSaved: () => true,
        markSessionFinished: finished,
        markSendAttempted: vi.fn<() => void>(),
        onValidationBlocked: vi.fn<() => void>(),
      });
    },
    { wrapper },
  );
  act(() => hook.result.current.requestSendAndArchive());
  await waitFor(() =>
    expect(toast.info).toHaveBeenCalledWith(
      "Waiting for the mail provider",
      expect.objectContaining({ description: expect.stringContaining(retryAt) }),
    ),
  );
  expect(sendComposeSession).toHaveBeenCalledTimes(1);
  expect(toast.success).not.toHaveBeenCalled();
  expect(archiveMessages).not.toHaveBeenCalled();
  expect(finished).not.toHaveBeenCalled();
  expect(onSent).not.toHaveBeenCalled();
  expect(navigate).not.toHaveBeenCalled();
  expect(emitSendEvent).not.toHaveBeenCalledWith(expect.objectContaining({ kind: "sent" }));
});
