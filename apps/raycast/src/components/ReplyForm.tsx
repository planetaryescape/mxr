import {
  Action,
  ActionPanel,
  Alert,
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const current = useRef<ReplyDraft>(undefined);
  // Once the daemon owns the reply (sent, scheduled or handed to the web
  // composer), leaving the form must not discard it.
  const handedOver = useRef(false);

  useEffect(() => {
    let closed = false;
    startReply(messageId, kind)
      .then((opened) => {
        if (closed) return void discardReply(opened).catch(() => undefined);
        current.current = opened;
        setDraft(opened);
        setBody(opened.session.bodyMarkdown);
      })
      .catch((failure) => setError(message(failure)));
    return () => {
      closed = true;
      const open = current.current;
      if (open && !handedOver.current) void discardReply(open).catch(() => undefined);
    };
  }, [messageId, kind]);

  async function submit() {
    if (!draft || busy) return;
    setBusy(true);
    const toast = await showToast({ style: Toast.Style.Animated, title: "Checking the reply…" });
    try {
      const written = await writeBody(draft, body);
      current.current = written;
      setDraft(written);
      const report = await checkSafety(written);
      await toast.hide();
      if (!(await confirmIssues(report))) return;

      if (report.allowed) {
        await scheduleWithUndo(written);
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
        await sendReply(written, override);
        handedOver.current = true;
        await showToast({ style: Toast.Style.Success, title: "Sent (no undo for an override)" });
      }
      onSent?.();
      pop();
    } catch (failure) {
      await showToast({
        style: Toast.Style.Failure,
        title: "Reply not sent",
        message: message(failure),
      });
    } finally {
      setBusy(false);
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
      setBody((existing) => `${suggestion.trim()}\n\n${existing}`);
      toast.style = Toast.Style.Success;
      toast.title = "Draft added above the quote";
    } catch (failure) {
      toast.style = Toast.Style.Failure;
      toast.title = "No draft";
      toast.message = message(failure);
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
          {webBase && (
            <Action.OpenInBrowser
              title="Open in mxr"
              url={`${webBase}${web.reply(messageId, kind === "reply_all" ? "all" : "single")}`}
              shortcut={Keyboard.Shortcut.Common.Open}
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
        onChange={setBody}
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
