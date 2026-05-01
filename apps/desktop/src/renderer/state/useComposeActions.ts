import { useEffectEvent, useRef } from "react";
import type { SetStateAction } from "react";
import type {
  ActionAckResponse,
  BridgeState,
  ComposeFrontmatter,
  ComposeSession,
  ComposeSessionKind,
  ComposeSessionResponse,
  FocusContext,
  SavedDraftSummary,
  WorkbenchScreen,
} from "../../shared/types";
import { fetchJson } from "./bridgeHttp";
import type { DesktopRequestCoordinator } from "./requestCoordinator";

type StateSetter<T> = (updater: SetStateAction<T>) => void;

type ComposeDraftSnapshot = {
  draftPath: string;
  accountId: string;
  editorCommand: string;
  kind: ComposeSessionKind;
  fingerprint: string;
  updatePayload: {
    draft_path: string;
    to: string;
    cc: string;
    bcc: string;
    subject: string;
    from: string;
    attach: string[];
    body: string;
  };
};

export function useComposeActions(props: {
  requestCoordinator: DesktopRequestCoordinator;
  bridge: BridgeState;
  composeSession: ComposeSession | null;
  composeDraft: ComposeFrontmatter | null;
  composeOpen: boolean;
  screen: WorkbenchScreen;
  setComposeSession: StateSetter<ComposeSession | null>;
  setComposeDraft: StateSetter<ComposeFrontmatter | null>;
  setComposeError: StateSetter<string | null>;
  setComposeBusy: StateSetter<string | null>;
  setComposeOpen: StateSetter<boolean>;
  setFocusContext: StateSetter<FocusContext>;
  showNotice: (message: string) => void;
  refreshCurrentView: (options?: { preserveReader?: boolean }) => Promise<void>;
}) {
  const composeSnapshotRef = useRef<string | null>(null);
  const composeBodyRef = useRef<string>("");

  const composeQueueKey = useEffectEvent((draftPath: string) => `compose:${draftPath}`);

  const closeComposeUi = useEffectEvent(() => {
    props.setComposeSession(null);
    props.setComposeDraft(null);
    props.setComposeOpen(false);
    props.setFocusContext(props.screen === "search" ? "search" : "mailList");
    composeSnapshotRef.current = null;
  });

  const currentComposeFingerprint = useEffectEvent(() => {
    if (!props.composeDraft) {
      return null;
    }
    return JSON.stringify(props.composeDraft) + "\u0000" + composeBodyRef.current;
  });

  const hydrateComposeSession = useEffectEvent((session: ComposeSession) => {
    composeSnapshotRef.current = JSON.stringify(session.frontmatter);
    composeBodyRef.current = session.bodyMarkdown || "";
    props.setComposeSession(session);
    props.setComposeDraft(session.frontmatter);
    props.setComposeError(null);
  });

  const captureComposeSnapshot = useEffectEvent((): ComposeDraftSnapshot | null => {
    if (props.bridge.kind !== "ready" || !props.composeSession || !props.composeDraft) {
      return null;
    }

    return {
      draftPath: props.composeSession.draftPath,
      accountId: props.composeSession.accountId,
      editorCommand: props.composeSession.editorCommand,
      kind: props.composeSession.kind,
      fingerprint: currentComposeFingerprint() ?? "",
      updatePayload: {
        draft_path: props.composeSession.draftPath,
        to: props.composeDraft.to,
        cc: props.composeDraft.cc,
        bcc: props.composeDraft.bcc,
        subject: props.composeDraft.subject,
        from: props.composeDraft.from,
        attach: props.composeDraft.attach,
        body: composeBodyRef.current,
      },
    };
  });

  const performPersistComposeDraft = useEffectEvent(
    async (snapshot: ComposeDraftSnapshot): Promise<ComposeSession> => {
      const bridge = props.bridge;
      if (bridge.kind !== "ready") {
        throw new Error("Bridge not ready");
      }

      const payload = await fetchJson<ComposeSessionResponse>(
        bridge.baseUrl,
        bridge.authToken,
        "/compose/session/update",
        {
          method: "POST",
          body: JSON.stringify(snapshot.updatePayload),
          requestLabel: "compose:update",
        },
      );

      return {
        ...payload.session,
        accountId: snapshot.accountId,
        editorCommand: snapshot.editorCommand,
        kind: snapshot.kind,
      };
    },
  );

  const persistComposeDraft = useEffectEvent(async () => {
    const snapshot = captureComposeSnapshot();
    if (!snapshot) {
      return props.composeSession;
    }

    const result = await props.requestCoordinator.queueComposeLatest(
      composeQueueKey(snapshot.draftPath),
      async () => await performPersistComposeDraft(snapshot),
    );

    if (result.status !== "committed") {
      return props.composeSession;
    }
    if (props.composeSession?.draftPath !== snapshot.draftPath) {
      return props.composeSession;
    }
    if (currentComposeFingerprint() !== snapshot.fingerprint) {
      return result.value;
    }

    hydrateComposeSession(result.value);
    return result.value;
  });

  const refreshComposeSession = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.composeSession) {
      return;
    }

    const draftPath = props.composeSession.draftPath;
    const fingerprintBefore = currentComposeFingerprint();

    const result = await props.requestCoordinator.queueComposeLatest(
      composeQueueKey(draftPath),
      async () =>
        await fetchJson<ComposeSessionResponse>(
          bridge.baseUrl,
          bridge.authToken,
          "/compose/session/refresh",
          {
            method: "POST",
            body: JSON.stringify({ draft_path: draftPath }),
            requestLabel: "compose:refresh",
          },
        ),
    );

    if (result.status !== "committed") {
      return;
    }
    if (props.composeSession?.draftPath !== draftPath) {
      return;
    }
    if (currentComposeFingerprint() !== fingerprintBefore) {
      return;
    }

    hydrateComposeSession({
      ...result.value.session,
      accountId: props.composeSession.accountId,
      editorCommand: props.composeSession.editorCommand,
      kind: props.composeSession.kind,
    });
  });

  const openDraftInEditorForSession = useEffectEvent(async (session: ComposeSession) => {
    props.setComposeBusy("Opening editor");
    props.setComposeError(null);
    try {
      await window.mxrDesktop.openDraftInEditor({
        draftPath: session.draftPath,
        editorCommand: session.editorCommand,
        cursorLine: session.cursorLine,
      });
      props.showNotice(`Opened ${session.editorCommand}`);
    } catch (error) {
      props.setComposeError(error instanceof Error ? error.message : "Failed to open editor");
    } finally {
      props.setComposeBusy(null);
    }
  });

  const openComposeShell = useEffectEvent(async (kind: ComposeSessionKind, messageId?: string) => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    if (props.composeSession && !props.composeOpen) {
      props.setComposeOpen(true);
      props.setFocusContext("compose");
      props.showNotice("Finish current draft before starting another");
      return;
    }
    if (props.composeSession && props.composeOpen) {
      props.showNotice("Finish current draft before starting another");
      return;
    }

    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<ComposeSessionResponse>(bridge.baseUrl, bridge.authToken, "/compose/session", {
        method: "POST",
        body: JSON.stringify({
          kind,
          message_id: messageId,
        }),
        requestLabel: "compose:create",
      }),
    );

    hydrateComposeSession(payload.session);
    props.setComposeOpen(true);
    props.setFocusContext("compose");
  });

  const openSavedDraft = useEffectEvent(async (draft: SavedDraftSummary) => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready") {
      return;
    }
    if (props.composeSession && !props.composeOpen) {
      props.setComposeOpen(true);
      props.setFocusContext("compose");
      props.showNotice("Finish current draft before starting another");
      return;
    }
    if (props.composeSession && props.composeOpen) {
      props.showNotice("Finish current draft before starting another");
      return;
    }

    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<ComposeSessionResponse>(
        bridge.baseUrl,
        bridge.authToken,
        "/compose/session/restore",
        {
          method: "POST",
          body: JSON.stringify({ draft_id: draft.id }),
          requestLabel: "compose:restore",
        },
      ),
    );

    hydrateComposeSession(payload.session);
    props.setComposeOpen(true);
    props.setFocusContext("compose");
    props.showNotice(`Resumed ${draft.subject || "saved draft"}`);
  });

  const closeComposeShell = useEffectEvent(() => {
    props.setComposeOpen(false);
    props.setComposeError(null);
    props.setComposeSession(null);
    props.setComposeDraft(null);
    props.setComposeBusy(null);
    composeSnapshotRef.current = null;
    props.setFocusContext(props.screen === "search" ? "search" : "mailList");
  });

  const submitComposeAction = useEffectEvent(
    async (path: "/compose/session/send" | "/compose/session/save", successMessage: string) => {
      const snapshot = captureComposeSnapshot();
      const bridge = props.bridge;
      if (bridge.kind !== "ready" || !snapshot) {
        return;
      }

      props.setComposeBusy(path === "/compose/session/send" ? "Sending" : "Saving");
      props.setComposeError(null);

      try {
        const result = await props.requestCoordinator.queueComposeLatest(
          composeQueueKey(snapshot.draftPath),
          async () => {
            const session = await performPersistComposeDraft(snapshot);
            await fetchJson<ActionAckResponse>(bridge.baseUrl, bridge.authToken, path, {
              method: "POST",
              body: JSON.stringify({
                draft_path: session.draftPath,
                account_id: session.accountId,
              }),
              requestLabel: path === "/compose/session/send" ? "compose:send" : "compose:save",
            });
            return session;
          },
        );
        if (result.status !== "committed") {
          return;
        }

        closeComposeUi();
        props.showNotice(successMessage);
        await props.refreshCurrentView({ preserveReader: true });
      } catch (error) {
        props.setComposeError(composeActionErrorMessage(error, path));
      } finally {
        props.setComposeBusy(null);
      }
    },
  );

  const discardComposeSession = useEffectEvent(async () => {
    const bridge = props.bridge;
    if (bridge.kind !== "ready" || !props.composeSession) {
      return;
    }

    const draftPath = props.composeSession.draftPath;
    props.setComposeBusy("Discarding");
    try {
      const result = await props.requestCoordinator.queueComposeLatest(
        composeQueueKey(draftPath),
        async () => {
          await fetchJson<ActionAckResponse>(
            bridge.baseUrl,
            bridge.authToken,
            "/compose/session/discard",
            {
              method: "POST",
              body: JSON.stringify({ draft_path: draftPath }),
              requestLabel: "compose:discard",
            },
          );
          return draftPath;
        },
      );
      if (result.status !== "committed") {
        return;
      }

      closeComposeUi();
      props.showNotice("Draft discarded");
    } catch (error) {
      props.setComposeError(error instanceof Error ? error.message : "Failed to discard draft");
    } finally {
      props.setComposeBusy(null);
    }
  });

  const launchComposeEditor = useEffectEvent(async () => {
    const session = await persistComposeDraft();
    if (!session) {
      return;
    }
    await openDraftInEditorForSession(session);
  });

  return {
    persistComposeDraft,
    refreshComposeSession,
    openComposeShell,
    openSavedDraft,
    closeComposeShell,
    submitComposeAction,
    discardComposeSession,
    launchComposeEditor,
    setComposeBody: (body: string) => {
      composeBodyRef.current = body;
    },
  };
}

function composeActionErrorMessage(
  error: unknown,
  path: "/compose/session/send" | "/compose/session/save",
) {
  const message = error instanceof Error ? error.message : "Compose action failed";
  if (path !== "/compose/session/send" || !isMacKeychainRepairError(message)) {
    return message;
  }

  const accountKey = extractMxrRepairAccountKey(message);
  const command = accountKey
    ? `mxr accounts repair ${accountKey}`
    : "mxr accounts repair <account>";
  return `Send failed: mxr cannot read this account password from macOS Keychain. Run \`${command}\`, then retry.`;
}

function isMacKeychainRepairError(message: string) {
  return (
    message.includes("requires interactive macOS keychain approval") ||
    message.includes("macOS keychain credential requires interactive approval")
  );
}

function extractMxrRepairAccountKey(message: string) {
  const match = message.match(/Password for mxr\/([^/\s]+)\/[^\s]+ requires interactive/u);
  const service = match?.[1];
  if (!service) {
    return null;
  }
  return service.replace(/-(smtp|imap)$/u, "");
}
