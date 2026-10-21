import { randomUUID } from "node:crypto";

import type { Page } from "@playwright/test";

import type { components } from "../../src/api/generated";
import { bridge } from "./state";

export type ArchiveOperation = components["schemas"]["ArchiveOperationData"];
type Workspace = components["schemas"]["ArchiveWorkspaceData"];

/** Exercise a second client's recoverable Archive path: review, submit, then query to a terminal receipt. */
export async function archiveViaBridge(
  page: Page,
  messageIds: string[],
): Promise<ArchiveOperation> {
  const { workspace } = await bridge<{ kind: string; workspace: Workspace }>(
    page,
    "/api/v1/mail/archive-operations/workspace",
  );
  const identity = {
    operation_id: randomUUID(),
    caller_id: randomUUID(),
    workspace,
  };
  const mutation = { mutation: "Archive", message_ids: messageIds };
  const { operation: reviewed } = await bridge<{ kind: string; operation: ArchiveOperation }>(
    page,
    "/api/v1/mail/archive-operations/preview",
    { identity, mutation },
  );
  let { operation } = await bridge<{ kind: string; operation: ArchiveOperation }>(
    page,
    "/api/v1/mail/archive-operations/submit",
    { identity, review_id: reviewed.review_id, mutation },
  );
  for (let attempt = 0; operation.running && attempt < 300; attempt += 1) {
    // Polls are sequential because each query observes one durable operation identity.
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 100));
    // The API keeps the operation identity stable while the worker finishes.
    // oxlint-disable-next-line no-await-in-loop
    ({ operation } = await bridge<{ kind: string; operation: ArchiveOperation }>(
      page,
      "/api/v1/mail/archive-operations/query",
      { identity },
    ));
  }
  if (operation.running) throw new Error("Archive operation stayed running for 30 seconds");
  return operation;
}
