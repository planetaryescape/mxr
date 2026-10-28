import {
  Action,
  ActionPanel,
  Detail,
  Form,
  Icon,
  Keyboard,
  Toast,
  useNavigation,
  showToast,
} from "@raycast/api";
import { useState } from "react";
import type { ComposeAssistProposal, ComposeModelChoice } from "../bridge/types";
import { message } from "./previewed";
import { escapeProofText, proposalMarkdown } from "./proofMarkdown";

interface Props {
  proposal: ComposeAssistProposal;
  onAccept: (proposal: ComposeAssistProposal) => void;
  onKeepMine: () => void;
  onRevisionPop: () => void;
  onRetry: (proposal: ComposeAssistProposal) => Promise<ComposeAssistProposal | undefined>;
  modelChoices?: ComposeModelChoice[];
  onRetryModel?: (
    proposal: ComposeAssistProposal,
    model: string,
  ) => Promise<ComposeAssistProposal | undefined>;
  onRevise: (
    proposal: ComposeAssistProposal,
    note: string,
  ) => Promise<ComposeAssistProposal | undefined>;
}

const ACCEPT: Keyboard.Shortcut = { modifiers: [], key: "enter" };
const KEEP_MINE: Keyboard.Shortcut = { modifiers: [], key: "escape" };
const RETRY: Keyboard.Shortcut = { modifiers: ["cmd"], key: "r" };

export function ComposeProof({
  proposal: initial,
  onAccept,
  onKeepMine,
  onRevisionPop,
  onRetry,
  modelChoices = [],
  onRetryModel,
  onRevise,
}: Props) {
  const { push } = useNavigation();
  const [proposal, setProposal] = useState(initial);
  const [loading, setLoading] = useState(false);

  async function retry() {
    if (loading) return;
    setLoading(true);
    try {
      const next = await onRetry(proposal);
      if (next) setProposal(next);
    } catch (failure) {
      await showToast({
        style: Toast.Style.Failure,
        title: "No new proof",
        message: message(failure),
      });
    } finally {
      setLoading(false);
    }
  }

  async function revise(note: string) {
    if (loading) return;
    setLoading(true);
    try {
      const next = await onRevise(proposal, note);
      if (next) {
        setProposal(next);
        return true;
      }
    } catch (failure) {
      await showToast({
        style: Toast.Style.Failure,
        title: "No revised proof",
        message: message(failure),
      });
    } finally {
      setLoading(false);
    }
    return false;
  }

  const provenance = proposal.provenance;
  const changed = proposal.yours_changed.map((index) => index + 1);
  const styleMarks = proposal.style_marks ?? [];
  const styleScore = proposal.style_score ?? 100;
  return (
    <Detail
      markdown={`${proposalMarkdown(proposal)}${styleMarks.length ? `\n\n---\n\n${styleMarks.map((mark) => `**${mark.name}**: ${escapeProofText(mark.context)}`).join("\n\n")}` : ""}`}
      isLoading={loading}
      metadata={
        <Detail.Metadata>
          <Detail.Metadata.Label title="Model" text={provenance.model} />
          <Detail.Metadata.Label title="Ran on" text={provenance.locality} />
          <Detail.Metadata.Separator />
          <Detail.Metadata.Label
            title="Writing history"
            text={provenance.history_used ? "Used" : "Not used"}
          />
          <Detail.Metadata.Label
            title="Voice samples"
            text={
              provenance.voice_examples?.length
                ? `${provenance.voice_examples.length} past email${provenance.voice_examples.length === 1 ? "" : "s"}`
                : "None"
            }
          />
          <Detail.Metadata.Separator />
          <Detail.Metadata.Label
            title="Your words"
            text={changed.length ? `Sentences ${changed.join(", ")}` : "Kept"}
          />
          <Detail.Metadata.Label
            title="Gaps"
            text={proposal.gaps.length ? proposal.gaps.map((gap) => gap.label).join(", ") : "None"}
          />
          <Detail.Metadata.Label
            title={`Style ${styleScore}/100`}
            text={
              styleMarks.length
                ? styleMarks.map((mark) => mark.name).join(", ")
                : "No detected patterns"
            }
          />
        </Detail.Metadata>
      }
      actions={
        <ActionPanel>
          <Action
            title="Accept Proposal"
            icon={Icon.Checkmark}
            shortcut={ACCEPT}
            onAction={() => {
              if (!loading) onAccept(proposal);
            }}
          />
          <Action title="Retry" icon={Icon.ArrowClockwise} shortcut={RETRY} onAction={retry} />
          {onRetryModel && modelChoices.length > 0 ? (
            <ActionPanel.Submenu title="Try Again with Another Model" icon={Icon.Gear}>
              {modelChoices.map((choice) => {
                return (
                  <Action
                    key={choice.id}
                    title={`${choice.label}${choice.cloud ? " · cloud" : " · this Mac"} · ${choice.sees}`}
                    onAction={async () => {
                      if (loading) return;
                      if (!choice.available) {
                        await showToast({
                          style: Toast.Style.Failure,
                          title: "Model unavailable",
                          message: choice.unavailable_reason ?? "Choose another model.",
                        });
                        return;
                      }
                      setLoading(true);
                      try {
                        const next = await onRetryModel(proposal, choice.id);
                        if (next) setProposal(next);
                      } catch (failure) {
                        await showToast({
                          style: Toast.Style.Failure,
                          title: "No new proof",
                          message: message(failure),
                        });
                      } finally {
                        setLoading(false);
                      }
                    }}
                  />
                );
              })}
            </ActionPanel.Submenu>
          ) : null}
          <Action
            title="Revise with Note"
            icon={Icon.Pencil}
            onAction={() => {
              if (!loading) push(<ComposeRevisionForm onSubmit={revise} />, onRevisionPop);
            }}
          />
          <Action title="Keep Mine" icon={Icon.Xmark} shortcut={KEEP_MINE} onAction={onKeepMine} />
        </ActionPanel>
      }
    />
  );
}

function ComposeRevisionForm({
  onSubmit,
}: {
  onSubmit: (note: string) => Promise<boolean | undefined>;
}) {
  const { pop } = useNavigation();
  const [note, setNote] = useState("");

  async function submit() {
    const instruction = note.trim();
    if (!instruction) {
      await showToast({ style: Toast.Style.Failure, title: "Add a note for the revision" });
      return;
    }
    if (await onSubmit(instruction)) pop();
  }

  return (
    <Form
      navigationTitle="Revise Proposal"
      actions={
        <ActionPanel>
          <Action.SubmitForm title="Revise" icon={Icon.Pencil} onSubmit={submit} />
        </ActionPanel>
      }
    >
      <Form.TextField id="note" title="Note" value={note} onChange={setNote} autoFocus />
    </Form>
  );
}
