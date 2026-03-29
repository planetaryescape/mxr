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
  WorkbenchScreen,
} from "../../shared/types";
import { fetchJson } from "./bridgeHttp";

type StateSetter<T> = (updater: SetStateAction<T>) => void;

export function useComposeActions(props: {
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

  const hydrateComposeSession = useEffectEvent((session: ComposeSession) => {
    composeSnapshotRef.current = JSON.stringify(session.frontmatter);
    composeBodyRef.current = session.bodyMarkdown || "";
    props.setComposeSession(session);
    props.setComposeDraft(session.frontmatter);
    props.setComposeError(null);
  });

  const persistComposeDraft = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.composeSession || !props.composeDraft) {
      console.log("[compose:persist] skipped — missing session or draft", {
        bridgeReady: props.bridge.kind === "ready",
        hasSession: Boolean(props.composeSession),
        hasDraft: Boolean(props.composeDraft),
      });
      return props.composeSession;
    }

    // Preserve fields the update response doesn't include
    const { accountId, editorCommand, kind } = props.composeSession;

    const updatePayload = {
      draft_path: props.composeSession.draftPath,
      to: props.composeDraft.to,
      cc: props.composeDraft.cc,
      bcc: props.composeDraft.bcc,
      subject: props.composeDraft.subject,
      from: props.composeDraft.from,
      attach: props.composeDraft.attach,
      body: composeBodyRef.current,
    };
    console.log("[compose:persist] sending update", updatePayload);

    const payload = await fetchJson<ComposeSessionResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/compose/session/update",
      { method: "POST", body: JSON.stringify(updatePayload) },
    );
    console.log("[compose:persist] update response", payload.session);

    // Merge preserved fields back — update response omits accountId/editorCommand/kind
    const mergedSession = { ...payload.session, accountId, editorCommand, kind };
    console.log("[compose:persist] merged session", { accountId, editorCommand, kind, draftPath: mergedSession.draftPath });
    hydrateComposeSession(mergedSession);
    return mergedSession;
  });

  const refreshComposeSession = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.composeSession) {
      return;
    }

    const payload = await fetchJson<ComposeSessionResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/compose/session/refresh",
      {
        method: "POST",
        body: JSON.stringify({ draft_path: props.composeSession.draftPath }),
      },
    );
    hydrateComposeSession(payload.session);
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
    if (props.bridge.kind !== "ready") {
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

    console.log("[compose:open] creating session", { kind, messageId });
    const payload = await fetchJson<ComposeSessionResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/compose/session",
      {
        method: "POST",
        body: JSON.stringify({
          kind,
          message_id: messageId,
        }),
      },
    );
    console.log("[compose:open] session created", {
      draftPath: payload.session.draftPath,
      accountId: payload.session.accountId,
      kind: payload.session.kind,
      editorCommand: payload.session.editorCommand,
      to: payload.session.frontmatter?.to,
    });

    hydrateComposeSession(payload.session);
    props.setComposeOpen(true);
    props.setFocusContext("compose");
    // Don't auto-launch external editor -- compose dialog has an integrated terminal
  });

  const closeComposeShell = useEffectEvent(() => {
    // Always fully close compose -- clear all state to prevent blank screens
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
      if (props.bridge.kind !== "ready" || !props.composeSession) {
        return;
      }

      console.log("[compose:submit] starting", { path });
      props.setComposeBusy(path === "/compose/session/send" ? "Sending" : "Saving");
      props.setComposeError(null);

      try {
        const session = await persistComposeDraft();
        if (!session) {
          console.log("[compose:submit] persist returned null — aborting");
          return;
        }
        const sendPayload = {
          draft_path: session.draftPath,
          account_id: session.accountId,
        };
        console.log("[compose:submit] sending to", path, sendPayload);
        await fetchJson<ActionAckResponse>(props.bridge.baseUrl, props.bridge.authToken, path, {
          method: "POST",
          body: JSON.stringify(sendPayload),
        });
        props.setComposeSession(null);
        props.setComposeDraft(null);
        composeSnapshotRef.current = null;
        props.setComposeOpen(false);
        props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        props.showNotice(successMessage);
        await props.refreshCurrentView({ preserveReader: true });
        console.log("[compose:submit] success");
      } catch (error) {
        console.error("[compose:submit] failed", error);
        props.setComposeError(error instanceof Error ? error.message : "Compose action failed");
      } finally {
        props.setComposeBusy(null);
      }
    },
  );

  const discardComposeSession = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.composeSession) {
      return;
    }

    props.setComposeBusy("Discarding");
    try {
      await fetchJson<ActionAckResponse>(
        props.bridge.baseUrl,
        props.bridge.authToken,
        "/compose/session/discard",
        {
          method: "POST",
          body: JSON.stringify({ draft_path: props.composeSession.draftPath }),
        },
      );
      props.setComposeSession(null);
      props.setComposeDraft(null);
      composeSnapshotRef.current = null;
      props.setComposeOpen(false);
      props.setFocusContext(props.screen === "search" ? "search" : "mailList");
      props.showNotice("Draft discarded");
    } catch (error) {
      props.setComposeError(error instanceof Error ? error.message : "Failed to discard draft");
    } finally {
      props.setComposeBusy(null);
    }
  });

  const launchComposeEditor = useEffectEvent(async () => {
    if (!props.composeSession) {
      return;
    }

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
    closeComposeShell,
    submitComposeAction,
    discardComposeSession,
    launchComposeEditor,
    setComposeBody: (body: string) => { composeBodyRef.current = body; },
  };
}
