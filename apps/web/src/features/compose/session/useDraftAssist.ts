/*
 * "Draft for me" in the compose session: one help line that sends the text
 * the user wrote to the daemon, and a proof that comes back as versions.
 * Nothing is written over the user's text until a proof is accepted, and
 * accepting is undone in one step until the text changes again.
 */

import { useMutation } from "@tanstack/react-query";
import { useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { addedSpans, mapModelSpans, NO_SPANS, type ModelSpan } from "../assist/modelSpans";
import { requestComposeAssist } from "../assist/api";
import { helpAction, helpPlaceholder, type HelpLineState } from "../assist/helpLine";
import { proofPieces } from "../assist/proof";
import { joinTextRegion, splitTextRegion, utf8Offset, type TextRegion } from "../assist/textRegion";
import type {
  ComposeAssistAction,
  ComposeAssistAdjust,
  ComposeAssistDraft,
  ComposeAssistProposal,
  ComposeAssistRequest,
} from "../assist/types";
import {
  addressList,
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
  beforeSpans: readonly ModelSpan[] | undefined;
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

/**
 * What the help is about. A reply or forward always names its message: a
 * stored draft's metadata can exist before the daemon has any copy, so the
 * message is the context. A stored draft is named only once a save has put
 * a revision on the daemon; a draft nobody has saved yet is a new message.
 */
function assistDraft(intent: ComposeIntent, draft: ComposeDraftState): ComposeAssistDraft {
  if (intent.messageId) {
    if (intent.kind === "forward") {
      return {
        kind: "forward",
        message_id: intent.messageId,
        to: addressList(draft.frontmatter.to),
      };
    }
    return {
      kind: "reply",
      message_id: intent.messageId,
      all: intent.kind === "reply_all",
    };
  }
  if (draft.draftId && draft.revision != null) {
    return { kind: "stored", draft_id: draft.draftId };
  }
  return {
    kind: "new",
    account_id: draft.accountId,
    to: addressList(draft.frontmatter.to),
    subject: draft.frontmatter.subject,
  };
}

/**
 * The editor's selection as a range inside the text region, or null when
 * the selection is empty or lies in the prefix or signature.
 */
export function selectedRegionRange(
  region: TextRegion | null,
  selection: BodySelection | null,
): BodySelection | null {
  if (!region || !selection || selection.from === selection.to) return null;
  const start = Math.min(selection.from, selection.to) - region.prefix.length;
  const end = Math.max(selection.from, selection.to) - region.prefix.length;
  if (start < 0 || end > region.text.length) return null;
  return { from: start, to: end };
}

function helpStateOf(
  region: TextRegion | null,
  selection: BodySelection | null,
  answersMessage: boolean,
): HelpLineState {
  return {
    hasText: (region?.text ?? "") !== "",
    hasSelection: selectedRegionRange(region, selection) !== null,
    answersMessage,
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
  const answersMessage = Boolean(intent.messageId);

  // The text region is split once per render; handlers split the live draft
  // when they run, so a keystroke between renders is never missed.
  const rendered = draftRef.current;
  const region = rendered ? splitTextRegion(rendered.bodyMarkdown) : null;
  const helpState = helpStateOf(region, selection, answersMessage);
  const version = versions[versionIndex] ?? null;
  const showingProof = proofOpen && version !== null;

  function helpStateNow(): HelpLineState {
    const current = draftRef.current;
    return helpStateOf(
      current ? splitTextRegion(current.bodyMarkdown) : null,
      selection,
      answersMessage,
    );
  }

  const inFlight = useRef(false);
  const ask = useMutation({
    mutationFn: async (input: { action: ComposeAssistAction; instruction: string }) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const live = splitTextRegion(current.bodyMarkdown);
      const to = firstAddress(current.frontmatter.to);
      if (!intent.messageId && !to) throw new Error("Add a recipient before drafting");
      const subject = current.frontmatter.subject.trim();
      if (!intent.messageId && !live.text.trim() && !input.instruction.trim() && !subject) {
        throw new Error("Say what this email should do");
      }
      const range = selectedRegionRange(live, selection);
      const request: ComposeAssistRequest = {
        draft: assistDraft(intent, current),
        text: live.text,
        action: input.action,
        adjust,
      };
      if (range) {
        request.selection = {
          start: utf8Offset(live.text, range.from),
          end: utf8Offset(live.text, range.to),
        };
      }
      const note = input.instruction.trim();
      if (note) request.instruction = note;
      const response = await requestComposeAssist(request);
      return {
        response,
        sentText: live.text,
        action: input.action,
        instruction: input.instruction,
      };
    },
    onSuccess: ({ response, sentText, action, instruction: asked }) => {
      const next: AssistVersion = {
        proposal: response.proposal,
        sentText,
        action,
        instruction: asked,
      };
      setProof((current) => ({
        versions: [...current.versions, next],
        index: current.versions.length,
        open: true,
      }));
    },
    onError: (error) => toast.error("Draft failed", { description: errorMessage(error) }),
  });

  function run(action: ComposeAssistAction, note: string) {
    // A ref, not `ask.isPending`: two presses before the next render both see
    // "idle" in the closure, and only the ref is set synchronously.
    if (inFlight.current) return;
    inFlight.current = true;
    ask.mutate({ action, instruction: note }, { onSettled: () => (inFlight.current = false) });
  }

  /** ↵ in the help line: finish, edit the selection, or write. */
  function submit() {
    run(helpAction(helpStateNow()), instruction);
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
   * Run the proof again. A note asks for a revision with the help line's
   * current action. With no note, a proof whose text has changed is asked
   * again with the action the live text calls for: the stored action was
   * chosen for text that is no longer there. An unchanged proof repeats its
   * own request.
   */
  function again(note?: string) {
    if (!version) return;
    const trimmed = note?.trim() ?? "";
    const current = draftRef.current;
    const stale = version.sentText !== (current ? splitTextRegion(current.bodyMarkdown).text : "");
    if (trimmed || stale) run(helpAction(helpStateNow()), trimmed || version.instruction);
    else run(version.action, version.instruction);
  }

  function accept() {
    const current = draftRef.current;
    if (!current || !version) return;
    const live = splitTextRegion(current.bodyMarkdown);
    if (live.text !== version.sentText) return;
    const { body, textStart } = joinTextRegion(live, version.proposal.text);
    const inserted = version.proposal.text.trimEnd().length;
    const spans = mapModelSpans(current.modelSpans ?? NO_SPANS, current.bodyMarkdown, body);
    const added = addedSpans(
      proofPieces(version.proposal.changes, version.proposal.gaps),
      textStart,
      inserted,
    );
    const modelSpans = added.length > 0 ? [...spans, ...added] : spans;
    setUndo({ before: current.bodyMarkdown, beforeSpans: current.modelSpans, after: body });
    setDraft((draft) => (draft ? { ...draft, bodyMarkdown: body, modelSpans } : draft));
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
      : helpPlaceholder(helpState),
    submit,
    polish,
    again,
    generating: ask.isPending,
    version,
    versionCount: versions.length,
    versionIndex,
    stepVersion,
    proofOpen: showingProof,
    stale: version !== null && version.sentText !== (region?.text ?? ""),
    accept,
    keepMine,
    canUndoAccept: undo !== null && draftRef.current?.bodyMarkdown === undo.after,
    undoAccept,
  };
}
