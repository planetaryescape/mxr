import { useEffectEvent } from "react";
import type { SetStateAction } from "react";
import type {
  ActionAckResponse,
  AttachmentFileResponse,
  BridgeState,
  BugReportResponse,
  ExportThreadResponse,
  FocusContext,
  MailboxRow,
  SidebarItem,
  SnoozePreset,
  ThreadBody,
  ThreadResponse,
} from "../../shared/types";
import { fetchJson } from "./bridgeHttp";
import type { DesktopRequestCoordinator } from "./requestCoordinator";

type StateSetter<T> = (updater: SetStateAction<T>) => void;

export function useMailboxDialogActions(props: {
  requestCoordinator: DesktopRequestCoordinator;
  bridge: BridgeState;
  screen: "mailbox" | "search" | "rules" | "accounts" | "diagnostics";
  layoutMode: "twoPane" | "threePane" | "fullScreen";
  selectedRow: MailboxRow | null;
  thread: ThreadResponse | null;
  effectiveSelection: string[];
  labelOptions: string[];
  selectedLabels: string[];
  customLabel: string;
  moveTargetLabel: string;
  selectedSnooze: string;
  jumpLabelOptions: SidebarItem[];
  jumpTargetLabel: string;
  threadLinks: string[];
  threadAttachments: Array<{
    id: string;
    filename: string;
    size_bytes: number;
    message_id: string;
  }>;
  setFocusContext: StateSetter<FocusContext>;
  setSelectedLabels: StateSetter<string[]>;
  setCustomLabel: StateSetter<string>;
  setLabelDialogOpen: StateSetter<boolean>;
  setMoveTargetLabel: StateSetter<string>;
  setMoveDialogOpen: StateSetter<boolean>;
  setSnoozePresets: StateSetter<SnoozePreset[]>;
  setSelectedSnooze: StateSetter<string>;
  setSnoozeDialogOpen: StateSetter<boolean>;
  setUnsubscribeDialogOpen: StateSetter<boolean>;
  setJumpTargetLabel: StateSetter<string>;
  setGoToLabelOpen: StateSetter<boolean>;
  setAttachmentDialogOpen: StateSetter<boolean>;
  setLinksDialogOpen: StateSetter<boolean>;
  setReportTitle: StateSetter<string>;
  setReportContent: StateSetter<string>;
  setReportOpen: StateSetter<boolean>;
  showNotice: (message: string) => void;
  runPendingMutation: (
    messageIds: string[],
    label: string,
    work: () => Promise<void>,
  ) => Promise<void>;
  refreshCurrentView: (options?: { preserveReader?: boolean }) => Promise<void>;
  closeReader: () => void;
  applySidebarLens: (item: SidebarItem) => Promise<void>;
  formatPendingMutationLabel: (verb: string, count: number, detail?: string) => string;
}) {
  const openApplyLabelDialog = useEffectEvent(() => {
    props.setSelectedLabels([]);
    props.setCustomLabel("");
    props.setLabelDialogOpen(true);
    props.setFocusContext("dialog");
  });

  const applyLabels = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.selectedRow) {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    const add = [
      ...props.selectedLabels,
      ...props.customLabel
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean),
    ];
    if (add.length === 0) {
      props.showNotice("Select at least one label");
      return;
    }

    try {
      await props.runPendingMutation(
        props.effectiveSelection,
        props.formatPendingMutationLabel("Applying labels to", props.effectiveSelection.length),
        async () => {
          await props.requestCoordinator.enqueueMutation(() =>
            fetchJson<ActionAckResponse>(baseUrl, authToken, "/mutations/labels", {
              method: "POST",
              body: JSON.stringify({
                message_ids: props.effectiveSelection,
                add,
                remove: [],
              }),
              requestLabel: "mutations:labels",
            }),
          );
          props.setLabelDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
          props.showNotice(`Applied ${add.length} label${add.length === 1 ? "" : "s"}`);
          await props.refreshCurrentView({ preserveReader: true });
        },
      );
    } catch (error) {
      props.showNotice(error instanceof Error ? error.message : "Failed to apply labels");
    }
  });

  const openMoveDialog = useEffectEvent(() => {
    props.setMoveTargetLabel(props.labelOptions[0] ?? "");
    props.setMoveDialogOpen(true);
    props.setFocusContext("dialog");
  });

  const moveSelectedMessage = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.selectedRow || !props.moveTargetLabel) {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    try {
      await props.runPendingMutation(
        props.effectiveSelection,
        props.formatPendingMutationLabel("Moving", props.effectiveSelection.length),
        async () => {
          await props.requestCoordinator.enqueueMutation(() =>
            fetchJson<ActionAckResponse>(baseUrl, authToken, "/mutations/move", {
              method: "POST",
              body: JSON.stringify({
                message_ids: props.effectiveSelection,
                target_label: props.moveTargetLabel,
              }),
              requestLabel: "mutations:move",
            }),
          );
          props.setMoveDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
          props.showNotice(`Moved to ${props.moveTargetLabel}`);
          await props.refreshCurrentView({ preserveReader: true });
        },
      );
    } catch (error) {
      props.showNotice(error instanceof Error ? error.message : "Failed to move messages");
    }
  });

  const openSnoozeDialog = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    const result = await props.requestCoordinator.runReplaceable(
      "actions:snooze-presets",
      ({ signal }) =>
        fetchJson<{ presets: SnoozePreset[] }>(baseUrl, authToken, "/actions/snooze/presets", {
          signal,
          requestLabel: "actions:snooze-presets",
        }),
    );
    if (result.status !== "committed") {
      return;
    }
    const payload = result.value;
    props.setSnoozePresets(payload.presets);
    props.setSelectedSnooze(payload.presets[0]?.id ?? "");
    props.setSnoozeDialogOpen(true);
    props.setFocusContext("dialog");
  });

  const snoozeSelectedMessage = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.selectedRow || !props.selectedSnooze) {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    try {
      await props.runPendingMutation(
        [props.selectedRow.id],
        props.formatPendingMutationLabel("Snoozing", 1),
        async () => {
          await props.requestCoordinator.enqueueMutation(() =>
            fetchJson<ActionAckResponse>(baseUrl, authToken, "/actions/snooze", {
              method: "POST",
              body: JSON.stringify({
                message_id: props.selectedRow?.id,
                until: props.selectedSnooze,
              }),
              requestLabel: "actions:snooze",
            }),
          );
          props.setSnoozeDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
          props.showNotice("Message snoozed");
          if (props.layoutMode !== "twoPane") {
            props.closeReader();
          }
          await props.refreshCurrentView();
        },
      );
    } catch (error) {
      props.showNotice(error instanceof Error ? error.message : "Failed to snooze message");
    }
  });

  const confirmUnsubscribe = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.selectedRow) {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    try {
      await props.runPendingMutation(
        [props.selectedRow.id],
        props.formatPendingMutationLabel("Unsubscribing", 1),
        async () => {
          await props.requestCoordinator.enqueueMutation(() =>
            fetchJson<ActionAckResponse>(baseUrl, authToken, "/actions/unsubscribe", {
              method: "POST",
              body: JSON.stringify({
                message_id: props.selectedRow?.id,
              }),
              requestLabel: "actions:unsubscribe",
            }),
          );
          props.setUnsubscribeDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
          props.showNotice(`Unsubscribed from ${props.selectedRow?.sender}`);
        },
      );
    } catch (error) {
      props.showNotice(error instanceof Error ? error.message : "Failed to unsubscribe");
    }
  });

  const openReport = useEffectEvent((title: string, content: string) => {
    props.setReportTitle(title);
    props.setReportContent(content);
    props.setReportOpen(true);
    props.setFocusContext("dialog");
  });

  const openExternalUrl = useEffectEvent(async (url: string) => {
    await window.mxrDesktop.openExternalUrl(url);
  });

  const openSelectedInBrowser = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready" || !props.selectedRow) {
      return;
    }
    const selectedRow = props.selectedRow;
    const thread = await loadThreadForBrowserOpen(props.bridge, props.requestCoordinator, {
      selectedRow,
      thread: props.thread,
    });
    const body =
      thread?.bodies.find((candidate) => candidate.message_id === selectedRow.id) ?? null;
    if (!thread || !body) {
      props.showNotice("No readable body available");
      return;
    }
    const html = buildBrowserDocument(thread.thread.subject, body);
    if (!html) {
      props.showNotice("No readable body available");
      return;
    }
    await window.mxrDesktop.openBrowserDocument({
      title: thread.thread.subject,
      html,
      suggestedFilename: `${thread.thread.id}.html`,
    });
    props.showNotice("Opened in browser");
  });

  const openLinksPanel = useEffectEvent(() => {
    if (props.threadLinks.length === 0) {
      props.showNotice("No links in this thread");
      return;
    }
    props.setLinksDialogOpen(true);
    props.setFocusContext("dialog");
  });

  const openAttachmentsPanel = useEffectEvent(() => {
    if (props.threadAttachments.length === 0) {
      props.showNotice("No attachments in this thread");
      return;
    }
    props.setAttachmentDialogOpen(true);
    props.setFocusContext("dialog");
  });

  const runAttachmentAction = useEffectEvent(
    async (
      path: "/attachments/open" | "/attachments/download",
      attachmentId: string,
      messageId: string,
    ) => {
      if (props.bridge.kind !== "ready") {
        return;
      }
      const { baseUrl, authToken } = props.bridge;
      const payload = await props.requestCoordinator.enqueueMutation(() =>
        fetchJson<AttachmentFileResponse>(baseUrl, authToken, path, {
          method: "POST",
          body: JSON.stringify({
            message_id: messageId,
            attachment_id: attachmentId,
          }),
          requestLabel: path.endsWith("open") ? "attachments:open" : "attachments:download",
        }),
      );
      props.showNotice(
        `${path.endsWith("open") ? "Opened" : "Downloaded"} ${payload.file.filename}`,
      );
    },
  );

  const openGoToLabelDialog = useEffectEvent(() => {
    props.setJumpTargetLabel(props.jumpLabelOptions[0]?.id ?? "");
    props.setGoToLabelOpen(true);
    props.setFocusContext("dialog");
  });

  const applyJumpTarget = useEffectEvent(async () => {
    const next = props.jumpLabelOptions.find((item) => item.id === props.jumpTargetLabel);
    if (!next) {
      return;
    }
    props.setGoToLabelOpen(false);
    await props.applySidebarLens(next);
  });

  const exportSelectedThread = useEffectEvent(async () => {
    const selectedRow = props.selectedRow;
    if (props.bridge.kind !== "ready" || !selectedRow) {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<ExportThreadResponse>(
        baseUrl,
        authToken,
        `/thread/${selectedRow.thread_id}/export`,
        { requestLabel: "thread:export" },
      ),
    );
    openReport("Thread export", payload.content);
  });

  const generateBugReport = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }
    const { baseUrl, authToken } = props.bridge;
    const payload = await props.requestCoordinator.enqueueMutation(() =>
      fetchJson<BugReportResponse>(baseUrl, authToken, "/diagnostics/bug-report", {
        requestLabel: "diagnostics:bug-report",
      }),
    );
    openReport("Bug report", payload.content);
  });

  return {
    openApplyLabelDialog,
    applyLabels,
    openMoveDialog,
    moveSelectedMessage,
    openSnoozeDialog,
    snoozeSelectedMessage,
    confirmUnsubscribe,
    openExternalUrl,
    openSelectedInBrowser,
    openLinksPanel,
    openAttachmentsPanel,
    runAttachmentAction,
    openGoToLabelDialog,
    applyJumpTarget,
    exportSelectedThread,
    generateBugReport,
  };
}

async function loadThreadForBrowserOpen(
  bridge: Extract<BridgeState, { kind: "ready" }>,
  requestCoordinator: DesktopRequestCoordinator,
  props: {
    selectedRow: MailboxRow | null;
    thread: ThreadResponse | null;
  },
) {
  const selectedRow = props.selectedRow;
  if (!selectedRow) {
    return null;
  }
  if (props.thread?.thread.id === selectedRow.thread_id) {
    return props.thread;
  }
  const result = await requestCoordinator.runReplaceable(
    `thread:browser-open:${selectedRow.thread_id}`,
    ({ signal }) =>
      fetchJson<ThreadResponse>(
        bridge.baseUrl,
        bridge.authToken,
        `/thread/${selectedRow.thread_id}`,
        {
          signal,
          requestLabel: "thread:browser-open",
        },
      ),
  );
  return result.status === "committed" ? result.value : null;
}

function buildBrowserDocument(subject: string, body: ThreadBody) {
  if (body.text_html) {
    return `<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(
      subject,
    )}</title></head><body>${body.text_html}</body></html>`;
  }
  if (body.text_plain) {
    return `<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(
      subject,
    )}</title></head><body><pre>${escapeHtml(body.text_plain)}</pre></body></html>`;
  }
  return null;
}

function escapeHtml(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
