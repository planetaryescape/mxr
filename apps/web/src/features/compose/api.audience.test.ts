/* @vitest-environment jsdom */
import { beforeEach, expect, test, vi } from "vitest";
import { apiFetch } from "@/api/client";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";
import { resetWorkspaceOwnerForTest } from "@/lib/workspaceOwner";
import { rememberComposeBinding, setComposeAudience } from "./api";

vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: vi.fn<typeof apiFetch>(),
}));

beforeEach(() => {
  resetWorkspaceOwnerForTest();
  vi.mocked(apiFetch).mockReset();
});

test("an audience response retains its workspace for the next save", async () => {
  const binding = {
    target: getBridgeBaseUrl(),
    workspace: { workspace_id: "reply-owner", recovery_generation: "generation" },
  };
  rememberComposeBinding("/private/reply.md", binding);
  vi.mocked(apiFetch).mockResolvedValue({
    session: { draftPath: "/private/reply.md", draftId: "reply", revision: 3 },
  });
  const response = await setComposeAudience({
    draftPath: "/private/reply.md",
    draftId: "reply",
    replyAll: true,
    expectedRevision: 2,
    dryRun: false,
  });
  expect(response.session.binding).toEqual(binding);
  expect(apiFetch).toHaveBeenCalledWith(
    "/api/v1/mail/compose/session/audience",
    expect.objectContaining({ binding }),
  );
});
