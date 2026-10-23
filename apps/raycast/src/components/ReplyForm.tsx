import {
  Action,
  ActionPanel,
  Alert,
  open,
  Form,
  Icon,
  Toast,
  confirmAlert,
  showToast,
  useNavigation,
} from "@raycast/api";
import { useEffect, useRef, useState } from "react";
import {
  type ReplyDraft,
  type ReplyKind,
  assistReply,
  cancelScheduled,
  checkSafety,
  discardReply,
  saveForWeb,
  scheduleReply,
  sendReply,
  startReply,
  writeBody,
} from "../bridge/compose";
import type { ComposeAssistProposal, SafetyReport } from "../bridge/types";
import {
  joinTextRegion,
  splitTextRegion,
} from "../../../web/src/features/compose/assist/textRegion";
import { ComposeProof } from "./ComposeProof";
import {
  OwnedRequestGate,
  hasUnfilledGap,
  proposalStillOwnsReply,
  shouldDiscardReply,
} from "./proposalLifecycle";
import { message } from "./previewed";
import { useWebBase, web } from "./web";
import { keyFor } from "./keys";

/** The daemon owns the wait, so closing Raycast does not lose the reply. */
const UNDO_SECONDS = 10;

interface Props {
  messageId: string;
  kind?: ReplyKind;
  /** Who the reply goes to, for the title while the draft opens. */
  to: string;
  onSent?: () => void;
}

export function ReplyForm({ messageId, kind = "reply", to, onSent }: Props) {
  const { pop, push } = useNavigation();
  const webBase = useWebBase();
  const [draft, setDraft] = useState<ReplyDraft>();
  const [body, setBody] = useState("");
  const [error, setError] = useState<string>();
  const current = useRef<ReplyDraft>(undefined);
  // What the form shows now; a submission compares it with what it checked.
  const latestBody = useRef("");
  const submissions = useRef(new OwnedRequestGate());
  const closed = useRef(false);
  // Once the daemon owns the reply (sent, scheduled or saved for the web
  // composer), leaving the form must not discard it.
  const handedOver = useRef(false);
  const keepDraft = useRef(false);
  const proposalOpen = useRef(false);
  const assistRequests = useRef(new OwnedRequestGate());

  const editBody = (text: string) => {
    latestBody.current = text;
    setBody(text);
  };

  const discardIfAbandoned = () => {
    const open = current.current;
    if (open && shouldDiscardReply(handedOver.current || keepDraft.current, proposalOpen.current)) {
      void discardReply(open).catch(() => undefined);
    }
  };

  useEffect(() => {
    closed.current = false;
    startReply(messageId, kind)
      .then((opened) => {
        current.current = opened;
        if (closed.current) return discardIfAbandoned();
        setDraft(opened);
        editBody(opened.session.bodyMarkdown);
      })
      .catch((failure) => setError(message(failure)));
    return () => {
      closed.current = true;
      const submissionInFlight = submissions.current.isRunning;
      submissions.current.cancel();
      assistRequests.current.cancel();
      // An in-flight write may have reached the daemon; preserve that draft.
      if (submissionInFlight) keepDraft.current = true;
      if (!submissionInFlight) discardIfAbandoned();
      proposalOpen.current = false;
      current.current = undefined;
    };
  }, [messageId, kind]);

  /** Save the shown body, unless a submission already owns the draft. */
  async function saveShownBody(
    expectedOwner?: ReplyDraft,
    stillOwned: () => boolean = () => true,
  ): Promise<{ written: ReplyDraft; text: string } | undefined> {
    const open = current.current;
    if (!open || (expectedOwner && open !== expectedOwner)) return undefined;
    const text = latestBody.current;
    const written = await writeBody(open, text);
    if ((expectedOwner && current.current !== expectedOwner) || !stillOwned()) return undefined;
    current.current = written;
    setDraft(written);
    return { written, text };
  }

  async function submit() {
    if (
      !current.current ||
      submissions.current.isRunning ||
      assistRequests.current.isRunning ||
      proposalOpen.current
    )
      return;
    const submission = submissions.current.begin();
    if (submission === undefined) return;
    const owner = current.current;
    let toast: Toast | undefined;
    try {
      toast = await showToast({ style: Toast.Style.Animated, title: "Checking the reply…" });
      const saved = await saveShownBody(
        owner,
        () => submissions.current.isCurrent(submission) && !closed.current,
      );
      if (!submissions.current.isCurrent(submission) || closed.current) return;
      if (!saved) return;
      const report = await checkSafety(saved.written);
      if (!submissions.current.isCurrent(submission) || closed.current) return;
      await toast.hide();
      if (hasUnfilledGap(report)) {
        await showToast({
          style: Toast.Style.Failure,
          title: "Fill in every gap before sending",
          message: report.issues.find((issue) => issue.code === "unfilled_gap")?.message,
        });
        return;
      }
      const confirmed = await confirmIssues(report);
      if (!submissions.current.isCurrent(submission) || closed.current || !confirmed) return;
      // The text area stays editable while the daemon checks; send only the
      // body that was checked and is still on screen.
      if (latestBody.current !== saved.text) {
        await showToast({
          style: Toast.Style.Failure,
          title: "The reply changed while it was checked",
          message: "Send again to check and send what is on screen.",
        });
        return;
      }

      if (!submissions.current.isCurrent(submission) || closed.current) return;
      if (report.allowed) {
        await scheduleWithUndo(saved.written);
      } else {
        // A scheduled send re-checks when it fires and has no override, so
        // an overridden reply goes now.
        const override = report.issues.find((issue) => issue.override_token)?.override_token;
        if (!override) {
          await showToast({
            style: Toast.Style.Failure,
            title: "mxr blocked this reply",
            message: report.issues.map((issue) => issue.message).join(" "),
          });
          return;
        }
        if (!submissions.current.isCurrent(submission) || closed.current) return;
        await sendReply(saved.written, override);
        handedOver.current = true;
        await showToast({ style: Toast.Style.Success, title: "Sent (no undo for an override)" });
      }
      if (submissions.current.isCurrent(submission)) onSent?.();
      if (submissions.current.isCurrent(submission) && !closed.current) pop();
    } catch (failure) {
      if (submissions.current.isCurrent(submission) && !closed.current) {
        await showToast({
          style: Toast.Style.Failure,
          title: "Reply not sent",
          message: message(failure),
        });
      }
    } finally {
      submissions.current.finish(submission);
      if (closed.current) discardIfAbandoned();
    }
  }

  async function scheduleWithUndo(written: ReplyDraft) {
    const draftId = await scheduleReply(written, new Date(Date.now() + UNDO_SECONDS * 1000));
    handedOver.current = true;
    await showToast({
      style: Toast.Style.Success,
      title: "Reply sends within a minute",
      primaryAction: {
        title: "Undo Send",
        shortcut: keyFor("mail.undo"),
        onAction: async (shown) => {
          shown.style = Toast.Style.Animated;
          shown.title = "Stopping…";
          try {
            await cancelScheduled(draftId);
            shown.style = Toast.Style.Success;
            shown.title = "Not sent; kept as a draft in mxr";
          } catch (failure) {
            // The daemon refuses a cancel once the send has started.
            shown.style = Toast.Style.Failure;
            shown.title = "Too late to stop it";
            shown.message = message(failure);
          }
        },
      },
    });
  }

  async function draftForMe() {
    const owner = current.current;
    if (
      !owner ||
      submissions.current.isRunning ||
      assistRequests.current.isRunning ||
      proposalOpen.current ||
      closed.current
    )
      return;
    const baseline = latestBody.current;
    const region = splitTextRegion(baseline);
    const request = assistRequests.current.begin();
    if (request === undefined) return;
    const workspaceKey = JSON.stringify(owner.workspace);
    let toast: Toast | undefined;
    try {
      toast = await showToast({
        style: Toast.Style.Animated,
        title: "Preparing a writing proof…",
      });
      const proposal = await assistReply(owner, messageId, kind, region.text);
      if (!assistRequests.current.isCurrent(request) || closed.current) {
        await toast.hide();
        return;
      }
      if (latestBody.current !== baseline) {
        toast.style = Toast.Style.Failure;
        toast.title = "The reply changed while the proof was prepared";
        toast.message = "Draft again to review the text now in the form.";
        return;
      }
      if (
        !proposalStillOwnsReply({
          closed: closed.current,
          currentDraft: current.current,
          owner,
          currentWorkspace: current.current ? JSON.stringify(current.current.workspace) : "",
          workspace: workspaceKey,
          currentBody: latestBody.current,
          baseline,
        })
      ) {
        await toast.hide();
        return;
      }
      await toast.hide();
      proposalOpen.current = true;
      push(
        <ComposeProof
          proposal={proposal}
          onAccept={(accepted) => {
            if (
              !proposalStillOwnsReply({
                closed: closed.current,
                currentDraft: current.current,
                owner,
                currentWorkspace: current.current ? JSON.stringify(current.current.workspace) : "",
                workspace: workspaceKey,
                currentBody: latestBody.current,
                baseline,
              })
            ) {
              void showToast({
                style: Toast.Style.Failure,
                title: "The reply changed",
                message: "Keep your text and prepare a new proof before accepting.",
              });
              return;
            }
            editBody(joinTextRegion(region, accepted.text).body);
            proposalOpen.current = false;
            assistRequests.current.cancel();
            pop();
          }}
          onKeepMine={() => {
            proposalOpen.current = false;
            assistRequests.current.cancel();
            pop();
          }}
          onRevisionPop={() => {
            assistRequests.current.cancel();
          }}
          onRetry={(shown) => requestAgain(owner, baseline, region, workspaceKey, shown.action)}
          onRevise={(shown, instruction) =>
            requestAgain(owner, baseline, region, workspaceKey, shown.action, {
              revise: shown.proposal_id,
              instruction,
            })
          }
        />,
        () => {
          proposalOpen.current = false;
          assistRequests.current.cancel();
        },
      );
    } catch (failure) {
      if (toast && assistRequests.current.isCurrent(request) && !closed.current) {
        toast.style = Toast.Style.Failure;
        toast.title = "No writing proof";
        toast.message = message(failure);
      } else if (toast) {
        await toast.hide();
      }
    } finally {
      assistRequests.current.finish(request);
    }
  }

  async function requestAgain(
    owner: ReplyDraft,
    baseline: string,
    region: ReturnType<typeof splitTextRegion>,
    workspaceKey: string,
    action: ComposeAssistProposal["action"],
    options: { revise: string; instruction: string } | undefined = undefined,
  ): Promise<ComposeAssistProposal | undefined> {
    if (submissions.current.isRunning || closed.current) return undefined;
    const request = assistRequests.current.begin();
    if (request === undefined) return undefined;
    try {
      const proposal = await assistReply(owner, messageId, kind, region.text, {
        action,
        ...(options ?? {}),
      });
      if (!assistRequests.current.isCurrent(request) || closed.current) {
        return undefined;
      }
      if (
        !proposalStillOwnsReply({
          closed: closed.current,
          currentDraft: current.current,
          owner,
          currentWorkspace: current.current ? JSON.stringify(current.current.workspace) : "",
          workspace: workspaceKey,
          currentBody: latestBody.current,
          baseline,
        })
      ) {
        await showToast({
          style: Toast.Style.Failure,
          title: "The reply changed",
          message: "The new proof was not shown. Prepare a proof for the text now in the form.",
        });
        return undefined;
      }
      return proposal;
    } finally {
      assistRequests.current.finish(request);
    }
  }

  /** Hand the reply as written to the web composer instead of starting over. */
  async function openInMxr() {
    if (
      !webBase ||
      submissions.current.isRunning ||
      assistRequests.current.isRunning ||
      proposalOpen.current ||
      closed.current
    )
      return;
    // Same guard as Send: one handover at a time, and leaving the form
    // waits for this one to settle before discarding anything.
    const submission = submissions.current.begin();
    if (submission === undefined) return;
    const owner = current.current;
    if (!owner) {
      submissions.current.finish(submission);
      return;
    }
    let toast: Toast | undefined;
    try {
      toast = await showToast({ style: Toast.Style.Animated, title: "Opening in mxr…" });
      const saved = await saveShownBody(
        owner,
        () => submissions.current.isCurrent(submission) && !closed.current,
      );
      if (!submissions.current.isCurrent(submission) || closed.current) return;
      if (!saved) return;
      const draftId = await saveForWeb(saved.written);
      handedOver.current = true;
      if (!submissions.current.isCurrent(submission) || closed.current) return;
      await open(`${webBase}${web.draft(draftId)}`);
      await toast.hide();
      if (submissions.current.isCurrent(submission) && !closed.current) pop();
    } catch (failure) {
      if (toast && submissions.current.isCurrent(submission) && !closed.current) {
        toast.style = Toast.Style.Failure;
        toast.title = "Could not open the reply in mxr";
        toast.message = message(failure);
      } else if (toast) {
        await toast.hide();
      }
    } finally {
      submissions.current.finish(submission);
      if (closed.current) discardIfAbandoned();
    }
  }

  const frontmatter = draft?.session.frontmatter;
  return (
    <Form
      navigationTitle={`Reply to ${to}`}
      isLoading={!draft && !error}
      actions={
        <ActionPanel>
          <Action.SubmitForm title="Send Reply" icon={Icon.Envelope} onSubmit={submit} />
          <Action title="Draft for Me" icon={Icon.Pencil} onAction={draftForMe} />
          {webBase && draft && (
            <Action
              title="Open in mxr"
              icon={Icon.Globe}
              shortcut={keyFor("list.open")}
              onAction={openInMxr}
            />
          )}
        </ActionPanel>
      }
    >
      {error && <Form.Description title="Error" text={error} />}
      {frontmatter && <Form.Description title="To" text={frontmatter.to} />}
      {frontmatter?.cc && <Form.Description title="Cc" text={frontmatter.cc} />}
      {frontmatter && <Form.Description title="Subject" text={frontmatter.subject} />}
      <Form.TextArea
        id="body"
        title="Reply"
        value={body}
        onChange={editBody}
        enableMarkdown
        autoFocus
      />
    </Form>
  );
}

/** Show what the daemon's check found; a blocker needs an explicit override. */
async function confirmIssues(report: SafetyReport): Promise<boolean> {
  if (report.issues.length === 0) return true;
  const blocked = !report.allowed;
  return confirmAlert({
    title: blocked ? "mxr would block this reply" : "Before you send",
    message: report.issues
      .map((issue) => `• ${issue.message}${issue.detail ? ` (${issue.detail})` : ""}`)
      .join("\n"),
    primaryAction: {
      title: blocked ? "Send Anyway" : "Send",
      style: blocked ? Alert.ActionStyle.Destructive : Alert.ActionStyle.Default,
    },
  });
}
