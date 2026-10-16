import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Check, RotateCcw } from "lucide-react";
import { useState } from "react";

import { apiFetch, BridgeRequestError } from "@/api/client";
import type { components } from "@/api/generated";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";

export type RecoveryOwner = components["schemas"]["ArchiveWorkspaceData"];
export type HeldDraft = components["schemas"]["RecoveredDraftData"];
export type RecoveryStatus = components["schemas"]["RecoveryStatusData"];
export type RecoveryResponse = Extract<
  components["schemas"]["ResponseData"],
  { kind: "RecoveryStatus" }
>;
export type RecoveryActionResponse = Extract<
  components["schemas"]["ResponseData"],
  { kind: "RecoveryAction" }
>;
type PreparedDraft = components["schemas"]["Draft"];
export type RecoveryActionRequest =
  | {
      path: "/api/v1/mail/recovery/activate";
      body: components["schemas"]["RecoveryActivateBody"];
    }
  | {
      path: "/api/v1/mail/recovery/prepare-draft";
      body: components["schemas"]["RecoveryPrepareBody"];
    };

const recoveryKey = ["recovery-status"];
export interface RecoveryApi {
  capability(): Promise<{ recovery_supported?: boolean }>;
  status(): Promise<RecoveryResponse>;
  action(request: RecoveryActionRequest): Promise<RecoveryActionResponse>;
}

const bridgeRecoveryApi: RecoveryApi = {
  capability: () => apiFetch<{ recovery_supported?: boolean }>("/api/v1/admin/status"),
  status: () => apiFetch<RecoveryResponse>("/api/v1/mail/recovery/status"),
  action: ({ path, body }) => apiFetch<RecoveryActionResponse>(path, { method: "POST", body }),
};
interface ActivationPreview {
  owner: RecoveryOwner;
}
interface DraftPreview {
  owner: RecoveryOwner;
  draftId: string;
  revision: number;
  draft: PreparedDraft;
}

function sameOwner(a: RecoveryOwner, b: RecoveryOwner): boolean {
  return a.workspace_id === b.workspace_id && a.recovery_generation === b.recovery_generation;
}

function addressList(addresses: PreparedDraft["to"]): string {
  return (
    addresses.map(({ name, email }) => (name ? `${name} <${email}>` : email)).join(", ") || "(none)"
  );
}

function readableBody(draft: PreparedDraft): string {
  if (draft.body_markdown?.trim()) return draft.body_markdown;
  if (draft.body_text?.trim()) return draft.body_text;
  if (draft.body_html?.trim()) {
    const document = new DOMParser().parseFromString(draft.body_html, "text/html");
    document.querySelectorAll("script, style").forEach((element) => element.remove());
    return document.body.textContent?.trim() || "(empty body)";
  }
  return "(empty body)";
}

async function fetchRecovery(api: RecoveryApi): Promise<RecoveryStatus | null> {
  const capability = await api.capability();
  if (capability.recovery_supported !== true) return null;
  const response = await api.status();
  return response.kind === "RecoveryStatus" ? response.recovery : null;
}

export function isRestoredProfile(recovery: RecoveryStatus | null | undefined): boolean {
  return Boolean(recovery && (recovery.restore_pending || recovery.source_owner_stopped));
}

export function useRecoveryStatus(api: RecoveryApi = bridgeRecoveryApi) {
  return useQuery({
    queryKey: recoveryKey,
    queryFn: () => fetchRecovery(api),
    staleTime: 15_000,
    refetchInterval: 15_000,
    retry: false,
  });
}

function isProtocolMismatch(error: unknown): boolean {
  return (
    error instanceof BridgeRequestError &&
    (error.code === "protocol_mismatch" || error.status === 426)
  );
}

export function RecoveryNotice({ api = bridgeRecoveryApi }: { api?: RecoveryApi } = {}) {
  const queryClient = useQueryClient();
  const status = useRecoveryStatus(api);
  const [ownerStoppedFor, setOwnerStoppedFor] = useState<RecoveryOwner | null>(null);
  const [activationPreview, setActivationPreview] = useState<ActivationPreview | null>(null);
  const [draftPreview, setDraftPreview] = useState<DraftPreview | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const action = useMutation({
    mutationFn: (request: RecoveryActionRequest) => api.action(request),
    onSuccess: async (result, variables) => {
      setMessage(
        result.action.dry_run
          ? "Review this preview, then confirm explicitly."
          : "Recovery action completed.",
      );
      if (result.action.dry_run && variables.path === "/api/v1/mail/recovery/activate") {
        setActivationPreview({ owner: variables.body.expected_workspace });
      }
      if (
        result.action.dry_run &&
        variables.path === "/api/v1/mail/recovery/prepare-draft" &&
        result.action.prepared_draft
      ) {
        setDraftPreview({
          owner: variables.body.expected_workspace,
          draftId: variables.body.draft_id,
          revision: variables.body.expected_revision,
          draft: result.action.prepared_draft,
        });
      }
      if (!result.action.dry_run) {
        setActivationPreview(null);
        setDraftPreview(null);
        await queryClient.invalidateQueries({ queryKey: recoveryKey });
      }
    },
  });
  const recovery = status.data;
  // The "needs a protocol upgrade" card is for a daemon that answered with a
  // mismatch. A stopped daemon is the offline banner's to say, and any other
  // failed poll retries on its own every 15 seconds without a banner.
  const protocolFailure = isProtocolMismatch(status.error);
  if (
    !recovery?.restore_pending &&
    !recovery?.held_drafts.length &&
    !recovery?.retained_intentions.length &&
    !protocolFailure
  )
    return null;
  const currentOwner = recovery
    ? { workspace_id: recovery.workspace_id, recovery_generation: recovery.recovery_generation }
    : null;
  const ownerStopped = Boolean(
    ownerStoppedFor && currentOwner && sameOwner(ownerStoppedFor, currentOwner),
  );
  const currentActivationPreview =
    activationPreview && currentOwner && sameOwner(activationPreview.owner, currentOwner)
      ? activationPreview
      : null;
  const currentDraftPreview =
    draftPreview &&
    currentOwner &&
    sameOwner(draftPreview.owner, currentOwner) &&
    recovery?.held_drafts.some(
      (draft) =>
        draft.draft_id === draftPreview.draftId && draft.revision === draftPreview.revision,
    )
      ? draftPreview
      : null;

  return (
    <section
      className="mx-3 mt-3 rounded-md border border-warning/50 bg-warning/10 p-4 text-sm"
      aria-labelledby="recovery-notice-title"
    >
      <div className="flex items-start gap-3">
        <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-3">
          <div>
            <h2 id="recovery-notice-title" className="font-semibold">
              {protocolFailure
                ? "Recovery status unavailable"
                : recovery?.restore_pending
                  ? "Restored profile is paused"
                  : "Restored profile recovery"}
            </h2>
            <p className="mt-1 text-muted-foreground">
              {protocolFailure
                ? "The selected daemon may need a protocol upgrade before recovery status is available. No retained work was activated."
                : "Old queued work will not replay. A backup may predate a send; missing evidence does not prove the provider did not accept it. Original drafts stay held, including after activation."}
            </p>
          </div>
          {recovery ? (
            <>
              <p>
                {recovery.retained_intentions.length} retained intention(s),{" "}
                {recovery.held_drafts.length} held original draft(s), {recovery.retired_intentions}{" "}
                retired intention(s).
              </p>
              {recovery.retained_intentions.length > 0 ? (
                <ul className="list-disc space-y-1 pl-5">
                  {recovery.retained_intentions.map((intent) => (
                    <li key={`${intent.kind}:${intent.original_id}`}>
                      {intent.kind}: {intent.original_id} — {intent.original_state}
                      {intent.outcome_reference ? ` (${intent.outcome_reference})` : ""}
                      {intent.archive_evidence ? (
                        <pre className="mt-1 whitespace-pre-wrap">
                          {JSON.stringify(intent.archive_evidence, null, 2)}
                        </pre>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : null}
              {recovery.held_drafts.map((draft) => (
                <div
                  key={draft.draft_id}
                  className="flex flex-wrap items-center gap-2 rounded-sm border border-border/60 p-2"
                >
                  <span className="min-w-0 flex-1">
                    <strong>{draft.subject || "(no subject)"}</strong>
                    <span className="ml-2 text-muted-foreground">
                      original {draft.draft_id}, revision {draft.revision}
                    </span>
                  </span>
                  {draft.prepared_draft_id ? (
                    <span>New draft: {draft.prepared_draft_id}</span>
                  ) : recovery.source_owner_stopped &&
                    !recovery.restore_pending &&
                    draftPreview?.draftId === draft.draft_id &&
                    currentOwner &&
                    sameOwner(draftPreview.owner, currentOwner) &&
                    draftPreview.revision === draft.revision ? (
                    <Button
                      size="sm"
                      disabled={action.isPending}
                      onClick={() =>
                        action.mutate({
                          path: "/api/v1/mail/recovery/prepare-draft",
                          body: {
                            expected_workspace: draftPreview.owner,
                            draft_id: draftPreview.draftId,
                            expected_revision: draftPreview.revision,
                            confirm: true,
                          },
                        })
                      }
                    >
                      Confirm new draft
                    </Button>
                  ) : recovery.source_owner_stopped && !recovery.restore_pending && currentOwner ? (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={action.isPending}
                      onClick={() =>
                        action.mutate({
                          path: "/api/v1/mail/recovery/prepare-draft",
                          body: {
                            expected_workspace: currentOwner,
                            draft_id: draft.draft_id,
                            expected_revision: draft.revision,
                            confirm: false,
                          },
                        })
                      }
                    >
                      Preview new draft
                    </Button>
                  ) : null}
                </div>
              ))}
              {currentDraftPreview ? (
                <div
                  className="space-y-2 rounded-sm border border-border p-3"
                  aria-label="Prepared draft preview"
                >
                  <p className="font-medium">
                    Review the new draft before confirming. The original draft and its evidence
                    remain held.
                  </p>
                  <p>
                    To: {addressList(currentDraftPreview.draft.to)}
                    <br />
                    Cc: {addressList(currentDraftPreview.draft.cc)}
                    <br />
                    Bcc: {addressList(currentDraftPreview.draft.bcc)}
                    <br />
                    Subject: {currentDraftPreview.draft.subject || "(no subject)"}
                  </p>
                  <pre className="max-h-48 overflow-auto whitespace-pre-wrap">
                    {readableBody(currentDraftPreview.draft)}
                  </pre>
                  {currentDraftPreview.draft.managed_attachments?.length ? (
                    <ul className="list-disc pl-5">
                      {currentDraftPreview.draft.managed_attachments.map((file) => (
                        <li key={`${file.filename}:${file.sha256}`}>
                          {file.filename} · {file.length_bytes} bytes · SHA-256 {file.sha256}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                  {currentDraftPreview.draft.attachments.length ? (
                    <p>Legacy attachments: {currentDraftPreview.draft.attachments.join(", ")}</p>
                  ) : null}
                  <p className="text-muted-foreground">
                    Confirming creates a separate local draft for normal review. It does not send.
                  </p>
                </div>
              ) : draftPreview ? (
                <p role="alert">
                  The selected owner or original revision changed. Preview the current draft again
                  before confirming.
                </p>
              ) : null}
              {recovery.restore_pending ? (
                <div className="space-y-2 border-t border-warning/30 pt-3">
                  <div className="flex items-start gap-2">
                    <Checkbox
                      id="source-owner-stopped"
                      checked={ownerStopped}
                      onCheckedChange={(value) => {
                        setOwnerStoppedFor(value === true ? currentOwner : null);
                        setActivationPreview(null);
                      }}
                    />
                    <Label htmlFor="source-owner-stopped">
                      I stopped the previous daemon owner and will keep it stopped.
                    </Label>
                  </div>
                  {currentActivationPreview ? (
                    <Button
                      size="sm"
                      disabled={!ownerStopped || action.isPending}
                      onClick={() =>
                        action.mutate({
                          path: "/api/v1/mail/recovery/activate",
                          body: {
                            expected_workspace: currentActivationPreview.owner,
                            source_owner_stopped: true,
                            confirm: true,
                          },
                        })
                      }
                    >
                      <Check aria-hidden="true" className="mr-1 size-4" />
                      Confirm activation
                    </Button>
                  ) : currentOwner ? (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!ownerStopped || action.isPending || !currentOwner}
                      onClick={() =>
                        action.mutate({
                          path: "/api/v1/mail/recovery/activate",
                          body: {
                            expected_workspace: currentOwner,
                            source_owner_stopped: true,
                            confirm: false,
                          },
                        })
                      }
                    >
                      <RotateCcw aria-hidden="true" className="mr-1 size-4" />
                      Preview activation
                    </Button>
                  ) : null}
                  {activationPreview && !currentActivationPreview ? (
                    <p role="alert">
                      The selected workspace changed. Preview activation again before confirming.
                    </p>
                  ) : null}
                </div>
              ) : null}
              {message ? (
                <p role="status" className="text-muted-foreground">
                  {message}
                </p>
              ) : null}
              {action.isError ? (
                <p role="alert" className="text-destructive">
                  {action.error.message}
                </p>
              ) : null}
            </>
          ) : null}
        </div>
      </div>
    </section>
  );
}
