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

  const hydrateComposeSession = useEffectEvent((session: ComposeSession) => {
    composeSnapshotRef.current = JSON.stringify(session.frontmatter);
    props.setComposeSession(session);
    props.setComposeDraft(session.frontmatter);
    props.setComposeError(null);
  });

  const persistComposeDraft = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.composeSession || !props.composeDraft) {
      return props.composeSession;
    }

    const snapshot = JSON.stringify(props.composeDraft);
    if (composeSnapshotRef.current === snapshot) {
      return props.composeSession;
    }

    const payload = await fetchJson<ComposeSessionResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/compose/session/update",
      {
        method: "POST",
        body: JSON.stringify({
          draft_path: props.composeSession.draftPath,
          to: props.composeDraft.to,
          cc: props.composeDraft.cc,
          bcc: props.composeDraft.bcc,
          subject: props.composeDraft.subject,
          from: props.composeDraft.from,
          attach: props.composeDraft.attach,
        }),
      },
    );
    hydrateComposeSession(payload.session);
    return payload.session;
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

    hydrateComposeSession(payload.session);
    props.setComposeOpen(true);
    props.setFocusContext("compose");
  });

  const closeComposeShell = useEffectEvent(() => {
    props.setComposeOpen(false);
    props.setComposeError(null);
    props.setFocusContext(props.screen === "search" ? "search" : "mailList");
    props.showNotice("Draft hidden. Resume it from the header.");
  });

  const submitComposeAction = useEffectEvent(
    async (path: "/compose/session/send" | "/compose/session/save", successMessage: string) => {
      if (props.bridge.kind !== "ready" || !props.composeSession) {
        return;
      }

      props.setComposeBusy(path === "/compose/session/send" ? "Sending" : "Saving");
      props.setComposeError(null);

      try {
        const session = await persistComposeDraft();
        if (!session) {
          return;
        }
        await fetchJson<ActionAckResponse>(props.bridge.baseUrl, props.bridge.authToken, path, {
          method: "POST",
          body: JSON.stringify({
            draft_path: session.draftPath,
            account_id: session.accountId,
          }),
        });
        props.setComposeSession(null);
        props.setComposeDraft(null);
        composeSnapshotRef.current = null;
        props.setComposeOpen(false);
        props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        props.showNotice(successMessage);
        await props.refreshCurrentView({ preserveReader: true });
      } catch (error) {
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

    props.setComposeBusy("Opening editor");
    try {
      const session = await persistComposeDraft();
      if (!session) {
        return;
      }
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

  return {
    persistComposeDraft,
    refreshComposeSession,
    openComposeShell,
    closeComposeShell,
    submitComposeAction,
    discardComposeSession,
    launchComposeEditor,
  };
}
