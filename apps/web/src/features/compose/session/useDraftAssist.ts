/*
 * "Draft for me" in the compose session: one help line that sends the text
 * the user wrote to the daemon, and a proof that comes back as versions.
 * Nothing is written over the user's text until a proof is accepted, and
 * accepting is undone in one step until the text changes again.
 */

import { useMutation } from "@tanstack/react-query";
import { useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { apiFetch } from "@/api/client";
import { helpAction, helpPlaceholder, type HelpLineState } from "../assist/helpLine";
import { mapModelSpans } from "../assist/modelSpans";
import { proofPieces } from "../assist/proof";
import { joinTextRegion, splitTextRegion, utf8Offset } from "../assist/textRegion";
import type {
  ComposeAssistAdjust,
  ComposeAssistAction,
  ComposeAssistDraft,
  ComposeAssistProposal,
  ComposeAssistRequest,
  ComposeAssistResponse,
} from "../assist/types";
import {
  errorMessage,
  firstAddress,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";

interface DraftAssistInput {
  intent: ComposeIntent;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  setDraft: Dispatch<SetStateAction<ComposeDraftState | null>>;
  setDirty: Dispatch<SetStateAction<boolean>>;
}

/** One answer from the daemon, with what it was asked about. */
export interface AssistVersion {
  proposal: ComposeAssistProposal;
  /** The text region the proof was made from; a mismatch makes it stale. */
  sentText: string;
  action: ComposeAssistAction;
  instruction: string;
}

/** What accepting replaced, so ⌘Z can put it back in one step. */
interface AcceptUndo {
  before: string;
  beforeSpans: ComposeDraftState["modelSpans"];
  after: string;
}

interface ProofState {
  versions: AssistVersion[];
  index: number;
  open: boolean;
}

const EMPTY_PROOF: ProofState = { versions: [], index: 0, open: false };

/** A selection in the body, in UTF-16 units as the editor reports it. */
export interface BodySelection {
  from: number;
  to: number;
}

const NO_ADJUST: ComposeAssistAdjust = {
  shorter: false,
  warmer: false,
  more_formal: false,
  proofread: false,
};

function assistDraft(intent: ComposeIntent, draft: ComposeDraftState): ComposeAssistDraft {
  if (draft.draftId) return { kind: "stored", draft_id: draft.draftId };
  if (intent.messageId) {
    if (intent.kind === "forward") {
      return { kind: "forward", message_id: intent.messageId, to: draft.frontmatter.to };
    }
    return {
      kind: "reply",
      message_id: intent.messageId,
      all: intent.kind === "reply_all",
    };
  }
  return {
    kind: "new",
    account_id: draft.accountId,
    to: draft.frontmatter.to,
    subject: draft.frontmatter.subject,
  };
}

export function useDraftAssist({ intent, draftRef, setDraft, setDirty }: DraftAssistInput) {
  const [helpOpen, setHelpOpen] = useState(intent.openAssist === true);
  // Bumped by ⌘J and by opening the help line, so the line takes focus each time.
  const [focusNonce, setFocusNonce] = useState(intent.openAssist === true ? 1 : 0);
  const [instruction, setInstruction] = useState("");
  const [adjust, setAdjust] = useState<ComposeAssistAdjust>(NO_ADJUST);
  const [selection, setSelection] = useState<BodySelection | null>(null);
  // One state object: the proof's versions, which one is showing, and
  // whether the proof is on screen. Updated only through pure updaters.
  const [proof, setProof] = useState<ProofState>(EMPTY_PROOF);
  const { versions, index: versionIndex, open: proofOpen } = proof;
  const [undo, setUndo] = useState<AcceptUndo | null>(null);

  const draftState = (): HelpLineState => {
    const draft = draftRef.current;
    const region = draft ? splitTextRegion(draft.bodyMarkdown) : null;
    return {
      hasText: (region?.text ?? "") !== "",
      hasSelection: selectedRegionRange(draft?.bodyMarkdown ?? "", selection) !== null,
      answersMessage: Boolean(intent.messageId),
    };
  };

  const ask = useMutation({
    mutationFn: async (input: { action: ComposeAssistAction; instruction: string }) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const region = splitTextRegion(current.bodyMarkdown);
      const to = firstAddress(current.frontmatter.to);
      if (!intent.messageId && !to) throw new Error("Add a recipient before drafting");
      const subject = current.frontmatter.subject.trim();
      if (!intent.messageId && !region.text.trim() && !input.instruction.trim() && !subject) {
        throw new Error("Say what this email should do");
      }
      const range = selectedRegionRange(current.bodyMarkdown, selection);
      const request: ComposeAssistRequest = {
        draft: assistDraft(intent, current),
        text: region.text,
        ...(range
          ? {
              selection: {
                start: utf8Offset(region.text, range.from),
                end: utf8Offset(region.text, range.to),
              },
            }
          : {}),
        action: input.action,
        ...(input.instruction.trim() ? { instruction: input.instruction.trim() } : {}),
        adjust,
      };
      const response = await apiFetch<ComposeAssistResponse>("/api/v1/mail/compose/assist", {
        method: "POST",
        body: request,
      });
      return {
        response,
        sentText: region.text,
        action: input.action,
        instruction: input.instruction,
      };
    },
    onSuccess: ({ response, sentText, action, instruction: asked }) => {
      const version: AssistVersion = {
        proposal: response.proposal,
        sentText,
        action,
        instruction: asked,
      };
      setProof((current) => ({
        versions: [...current.versions, version],
        index: current.versions.length,
        open: true,
      }));
    },
    onError: (error) => toast.error("Draft failed", { description: errorMessage(error) }),
  });

  function run(action: ComposeAssistAction, note: string) {
    if (ask.isPending) return;
    ask.mutate({ action, instruction: note });
  }

  /** ↵ in the help line: finish, edit the selection, or write. */
  function submit() {
    run(helpAction(draftState()), instruction);
  }

  /** The Polish word: the one action that may change the user's words. */
  function polish() {
    run("polish", "");
  }

  function openHelp() {
    setHelpOpen(true);
    setFocusNonce((nonce) => nonce + 1);
  }

  function closeHelp() {
    setHelpOpen(false);
  }

  /**
   * Run the proof again. A note asks for a revision of the user's text with
   * the help line's current action; no note repeats the earlier request.
   */
  function again(note?: string) {
    const version = versions[versionIndex];
    if (!version) return;
    if (note?.trim()) run(helpAction(draftState()), note.trim());
    else run(version.action, version.instruction);
  }

  function accept() {
    const current = draftRef.current;
    const version = versions[versionIndex];
    if (!current || !version) return;
    const region = splitTextRegion(current.bodyMarkdown);
    if (region.text !== version.sentText) return;
    const { body, textStart } = joinTextRegion(region, version.proposal.text);
    const spans = mapModelSpans(current.modelSpans ?? [], current.bodyMarkdown, body);
    // Only the words the model added are marked; the user's own are not.
    let offset = 0;
    for (const piece of proofPieces(version.proposal.changes, version.proposal.gaps)) {
      if (piece.kind === "removed") continue;
      const length = piece.text.length;
      if (piece.kind === "added") {
        spans.push({ start: textStart + offset, end: textStart + offset + length });
      }
      offset += length;
    }
    setUndo({ before: current.bodyMarkdown, beforeSpans: current.modelSpans, after: body });
    setDraft((draft) => (draft ? { ...draft, bodyMarkdown: body, modelSpans: spans } : draft));
    setDirty(true);
    setProof(EMPTY_PROOF);
  }

  /** Esc: the user's text stays as it was; the proof goes. */
  function keepMine() {
    setProof(EMPTY_PROOF);
  }

  /** ⌘Z after an accept, while the text is still the accepted text. */
  function undoAccept(): boolean {
    const current = draftRef.current;
    if (!undo || !current || current.bodyMarkdown !== undo.after) return false;
    setDraft((draft) =>
      draft ? { ...draft, bodyMarkdown: undo.before, modelSpans: undo.beforeSpans } : draft,
    );
    setDirty(true);
    setUndo(null);
    return true;
  }

  function stepVersion(delta: number) {
    setProof((current) => {
      const next = current.index + delta;
      if (next < 0 || next >= current.versions.length) return current;
      return { ...current, index: next };
    });
  }

  const version = versions[versionIndex] ?? null;
  const currentText = draftRef.current ? splitTextRegion(draftRef.current.bodyMarkdown).text : "";
  const showingProof = proofOpen && version !== null;
  return {
    helpOpen,
    openHelp,
    closeHelp,
    focusNonce,
    instruction,
    setInstruction,
    adjust,
    toggleAdjust: (key: keyof ComposeAssistAdjust) =>
      setAdjust((current) => ({ ...current, [key]: !current[key] })),
    selection,
    setSelection,
    placeholder: showingProof
      ? "Say what to change, or press ↵ to try again"
      : helpPlaceholder(draftState()),
    submit,
    polish,
    again,
    generating: ask.isPending,
    version,
    versionCount: versions.length,
    versionIndex,
    stepVersion,
    proofOpen: showingProof,
    stale: version !== null && version.sentText !== currentText,
    accept,
    keepMine,
    canUndoAccept: undo !== null && draftRef.current?.bodyMarkdown === undo.after,
    undoAccept,
  };
}

/**
 * The editor's selection as a range inside the text region, or null when
 * the selection is empty or lies in the prefix or signature.
 */
export function selectedRegionRange(
  body: string,
  selection: BodySelection | null,
): BodySelection | null {
  if (!selection || selection.from === selection.to) return null;
  const region = splitTextRegion(body);
  const start = Math.min(selection.from, selection.to) - region.prefix.length;
  const end = Math.max(selection.from, selection.to) - region.prefix.length;
  if (start < 0 || end > region.text.length) return null;
  return { from: start, to: end };
}
