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
  Keyboard,
} from "@raycast/api";
import { useEffect, useRef, useState } from "react";
import {
  type ReplyDraft,
  type ReplyKind,
  cancelScheduled,
  checkSafety,
  discardReply,
  saveForWeb,
  scheduleReply,
  sendReply,
  startReply,
  suggestBody,
  writeBody,
} from "../bridge/compose";
import type { SafetyReport } from "../bridge/types";
import { message } from "./previewed";
import { useWebBase, web } from "./web";

/** The daemon owns the wait, so closing Raycast does not lose the reply. */
const UNDO_SECONDS = 10;

interface Props {
  messageId: string;
  threadId: string;
  kind?: ReplyKind;
  /** Who the reply goes to, for the title while the draft opens. */
  to: string;
  onSent?: () => void;
}

export function ReplyForm({ messageId, threadId, kind = "reply", to, onSent }: Props) {
  const { pop } = useNavigation();
  const webBase = useWebBase();
  const [draft, setDraft] = useState<ReplyDraft>();
  const [body, setBody] = useState("");
  const [error, setError] = useState<string>();
  const current = useRef<ReplyDraft>(undefined);
  // What the form shows now; a submission compares it with what it checked.
  const latestBody = useRef("");
  const submitting = useRef(false);
  const closed = useRef(false);
  // Once the daemon owns the reply (sent, scheduled or saved for the web
  // composer), leaving the form must not discard it.
  const handedOver = useRef(false);

  const editBody = (text: string) => {
    latestBody.current = text;
    setBody(text);
  };

  const discardIfAbandoned = () => {
    const open = current.current;
    if (open && !handedOver.current) void discardReply(open).catch(() => undefined);
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
      // A submission in flight decides the draft's fate when it settles.
      if (!submitting.current) discardIfAbandoned();
    };
  }, [messageId, kind]);

  /** Save the shown body, unless a submission already owns the draft. */
  async function saveShownBody(): Promise<{ written: ReplyDraft; text: string } | undefined> {
    const open = current.current;
    if (!open) return undefined;
    const text = latestBody.current;
    const written = await writeBody(open, text);
    current.current = written;
    setDraft(written);
    return { written, text };
  }

  async function submit() {
    if (!current.current || submitting.current) return;
    submitting.current = true;
    const toast = await showToast({ style: Toast.Style.Animated, title: "Checking the reply…" });
    try {
      const saved = await saveShownBody();
      if (!saved) return;
      const report = await checkSafety(saved.written);
      await toast.hide();
      if (!(await confirmIssues(report))) return;
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
        await sendReply(saved.written, override);
        handedOver.current = true;
        await showToast({ style: Toast.Style.Success, title: "Sent (no undo for an override)" });
      }
      onSent?.();
      if (!closed.current) pop();
    } catch (failure) {
      await showToast({
        style: Toast.Style.Failure,
        title: "Reply not sent",
        message: message(failure),
      });
    } finally {
      submitting.current = false;
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
        shortcut: { modifiers: ["cmd"], key: "z" },
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

  async function draftInMyVoice() {
    const toast = await showToast({
      style: Toast.Style.Animated,
      title: "Drafting in your voice…",
    });
    try {
      const suggestion = await suggestBody(
        threadId,
        "Reply to the latest message in this conversation.",
      );
      if (submitting.current || closed.current) {
        toast.style = Toast.Style.Failure;
        toast.title = "Draft not added: the reply is already being sent";
        return;
      }
      editBody(`${suggestion.trim()}\n\n${latestBody.current}`.trim());
      toast.style = Toast.Style.Success;
      toast.title = "Draft added above what you wrote";
    } catch (failure) {
      toast.style = Toast.Style.Failure;
      toast.title = "No draft";
      toast.message = message(failure);
    }
  }

  /** Hand the reply as written to the web composer instead of starting over. */
  async function openInMxr() {
    if (!webBase || submitting.current) return;
    // Same guard as Send: one handover at a time, and leaving the form
    // waits for this one to settle before discarding anything.
    submitting.current = true;
    const toast = await showToast({ style: Toast.Style.Animated, title: "Opening in mxr…" });
    try {
      const saved = await saveShownBody();
      if (!saved) return;
      const draftId = await saveForWeb(saved.written);
      handedOver.current = true;
      await open(`${webBase}${web.draft(draftId)}`);
      await toast.hide();
      if (!closed.current) pop();
    } catch (failure) {
      toast.style = Toast.Style.Failure;
      toast.title = "Could not open the reply in mxr";
      toast.message = message(failure);
    } finally {
      submitting.current = false;
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
          <Action
            title="Draft in My Voice"
            icon={Icon.Pencil}
            shortcut={{ modifiers: ["cmd"], key: "d" }}
            onAction={draftInMyVoice}
          />
          {webBase && draft && (
            <Action
              title="Open in mxr"
              icon={Icon.Globe}
              shortcut={Keyboard.Shortcut.Common.Open}
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
