/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import type { components } from "@/api/generated";
import {
  isRestoredProfile,
  RecoveryNotice,
  type RecoveryActionRequest,
  type RecoveryApi,
  type RecoveryStatus,
} from "./RecoveryNotice";

const owner = { workspace_id: "workspace-a", recovery_generation: "generation-2" };
let currentRecovery: RecoveryStatus = {
  ...owner,
  restore_pending: false,
  source_owner_stopped: true,
  retired_intentions: 1,
  retained_intentions: [
    {
      original_id: "intent-1",
      kind: "send",
      original_state: "unknown",
      archive_evidence: { operation_id: "archive-1", accepted: true },
    },
  ],
  held_drafts: [
    {
      draft_id: "draft-original",
      account_id: "acct-1",
      revision: 7,
      subject: "Re: project",
      prepared_draft_id: null,
    },
  ],
};
let preparedDraft: components["schemas"]["Draft"] = {
  id: "draft-new",
  account_id: "acct-1",
  created_at: "2026-10-12T10:00:00Z",
  updated_at: "2026-10-12T10:00:00Z",
  to: [{ name: "Ada", email: "ada@example.test" }],
  cc: [],
  bcc: [],
  subject: "Re: project",
  body_markdown: "Reviewed body",
  attachments: [],
  managed_attachments: [
    {
      account_id: "acct-1",
      draft_id: "draft-new",
      filename: "notes.txt",
      id: "attachment-1",
      length_bytes: 12,
      mime_type: "text/plain",
      sha256: "abc123",
    },
  ],
};

let actionRequests: RecoveryActionRequest[];
let recoveryApi: RecoveryApi;

function renderNotice(
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  render(
    <QueryClientProvider client={queryClient}>
      <RecoveryNotice api={recoveryApi} />
    </QueryClientProvider>,
  );
  return queryClient;
}

describe("RecoveryNotice", () => {
  test("treats a paused or activated restored profile as readable cached mail", () => {
    expect(isRestoredProfile(null)).toBe(false);
    expect(
      isRestoredProfile({ ...currentRecovery, restore_pending: true, source_owner_stopped: false }),
    ).toBe(true);
    expect(
      isRestoredProfile({ ...currentRecovery, restore_pending: false, source_owner_stopped: true }),
    ).toBe(true);
    expect(
      isRestoredProfile({
        ...currentRecovery,
        restore_pending: false,
        source_owner_stopped: false,
      }),
    ).toBe(false);
  });

  beforeEach(() => {
    preparedDraft = {
      id: "draft-new",
      account_id: "acct-1",
      created_at: "2026-10-12T10:00:00Z",
      updated_at: "2026-10-12T10:00:00Z",
      to: [{ name: "Ada", email: "ada@example.test" }],
      cc: [],
      bcc: [],
      subject: "Re: project",
      body_markdown: "Reviewed body",
      attachments: [],
      managed_attachments: [
        {
          account_id: "acct-1",
          draft_id: "draft-new",
          filename: "notes.txt",
          id: "attachment-1",
          length_bytes: 12,
          mime_type: "text/plain",
          sha256: "abc123",
        },
      ],
    };
    currentRecovery = {
      ...owner,
      restore_pending: false,
      source_owner_stopped: true,
      retired_intentions: 1,
      retained_intentions: [
        {
          original_id: "intent-1",
          kind: "send",
          original_state: "unknown",
          archive_evidence: { operation_id: "archive-1", accepted: true },
        },
      ],
      held_drafts: [
        {
          draft_id: "draft-original",
          account_id: "acct-1",
          revision: 7,
          subject: "Re: project",
          prepared_draft_id: null,
        },
      ],
    };
    actionRequests = [];
    recoveryApi = {
      capability: async () => ({ recovery_supported: true }),
      status: async () => ({ kind: "RecoveryStatus", recovery: currentRecovery }),
      action: async (request) => {
        actionRequests.push(request);
        return {
          kind: "RecoveryAction",
          action: {
            status: currentRecovery,
            prepared_draft: preparedDraft,
            dry_run: request.body.confirm === false,
          },
        };
      },
    };
  });
  afterEach(() => cleanup());

  test("keeps held drafts actionable after activation and confirms the exact preview owner and revision", async () => {
    renderNotice();
    expect(await screen.findByText(/"operation_id": "archive-1"/)).toBeVisible();
    fireEvent.click(await screen.findByRole("button", { name: "Preview new draft" }));
    expect(await screen.findByLabelText("Prepared draft preview")).toBeVisible();
    expect(screen.getByText(/To: Ada <ada@example.test>/)).toBeVisible();
    expect(screen.getByText("Reviewed body")).toBeVisible();
    expect(screen.getByText(/notes.txt · 12 bytes · SHA-256 abc123/)).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Confirm new draft" }));
    await waitFor(() =>
      expect(actionRequests).toContainEqual({
        path: "/api/v1/mail/recovery/prepare-draft",
        body: {
          expected_workspace: owner,
          draft_id: "draft-original",
          expected_revision: 7,
          confirm: true,
        },
      }),
    );
  });

  test("refuses confirmation when the original revision changes after preview", async () => {
    const queryClient = renderNotice();
    fireEvent.click(await screen.findByRole("button", { name: "Preview new draft" }));
    await screen.findByLabelText("Prepared draft preview");

    currentRecovery = {
      ...currentRecovery,
      held_drafts: [{ ...currentRecovery.held_drafts[0]!, revision: 8 }],
    };
    await queryClient.invalidateQueries({ queryKey: ["recovery-status"] });
    expect(await screen.findByRole("alert")).toHaveTextContent(/revision changed/i);
    expect(screen.queryByRole("button", { name: "Confirm new draft" })).not.toBeInTheDocument();
  });

  test("refuses confirmation when the selected workspace changes after preview", async () => {
    const queryClient = renderNotice();
    fireEvent.click(await screen.findByRole("button", { name: "Preview new draft" }));
    await screen.findByLabelText("Prepared draft preview");

    currentRecovery = { ...currentRecovery, workspace_id: "workspace-b" };
    await queryClient.invalidateQueries({ queryKey: ["recovery-status"] });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /owner or original revision changed/i,
    );
    expect(screen.queryByRole("button", { name: "Confirm new draft" })).not.toBeInTheDocument();
  });

  test("renders readable text from an HTML-only prepared draft", async () => {
    preparedDraft = {
      ...preparedDraft,
      body_markdown: null,
      body_text: null,
      body_html: "<p>Visible HTML-only content</p><script>hidden()</script>",
    };
    renderNotice();

    fireEvent.click(await screen.findByRole("button", { name: "Preview new draft" }));
    expect(await screen.findByText("Visible HTML-only content")).toBeInTheDocument();
    expect(screen.queryByText("hidden()")).not.toBeInTheDocument();
  });
});
