import { useMutation, useQuery } from "@tanstack/react-query";
import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import { toast } from "sonner";
import { apiFetch } from "@/api/client";
import type { components } from "@/api/generated";

import {
  addedSpans,
  carryModelSpans,
  keptRuns,
  mapModelSpans,
  NO_SPANS,
  type ModelSpan,
} from "../assist/modelSpans";
import { requestComposeAssist } from "../assist/api";
import { helpAction, helpPlaceholder, type HelpLineState } from "../assist/helpLine";
import { proofRuns } from "../assist/proof";
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

export interface AssistVersion {
  proposal: ComposeAssistProposal;
  sentText: string;
  action: ComposeAssistAction;
  instruction: string;
  model?: string;
  selection?: { start: number; end: number };
}

interface RevisionBaseline {
  proposalId?: string;
  sentText: string;
  selection?: AssistVersion["selection"];
}

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

const REPLY_OR_FORWARD = new Set(["reply", "reply_all", "forward"]);

function assistDraft(intent: ComposeIntent, draft: ComposeDraftState): ComposeAssistDraft {
  if (intent.messageId) {
    if (intent.kind === "forward") {
      return {
        kind: "forward",
        message_id: intent.messageId,
        to: addressList(draft.frontmatter.to),
      };
    }
    const storedReplyMode =
      draft.draftIntent === "reply" || draft.draftIntent === "reply_all"
        ? draft.draftIntent
        : undefined;
    return {
      kind: "reply",
      message_id: intent.messageId,
      all: storedReplyMode ? storedReplyMode === "reply_all" : intent.kind === "reply_all",
    };
  }
  if (
    draft.draftId &&
    draft.revision != null &&
    draft.draftIntent != null &&
    REPLY_OR_FORWARD.has(draft.draftIntent)
  ) {
    return { kind: "stored", draft_id: draft.draftId };
  }
  return {
    kind: "new",
    account_id: draft.accountId,
    to: addressList(draft.frontmatter.to),
    subject: draft.frontmatter.subject,
  };
}

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
  const [focusNonce, setFocusNonce] = useState(intent.openAssist === true ? 1 : 0);
  const [instruction, setInstruction] = useState("");
  const [adjust, setAdjust] = useState<ComposeAssistAdjust>(NO_ADJUST);
  const [selection, setSelection] = useState<BodySelection | null>(null);
  const [proof, setProof] = useState<ProofState>(EMPTY_PROOF);
  const { versions, index: versionIndex, open: proofOpen } = proof;
  const [undo, setUndo] = useState<AcceptUndo | null>(null);
  const [editorFocusNonce, setEditorFocusNonce] = useState(0);
  const answersMessage = Boolean(intent.messageId);
  const [model, setModel] = useState<string | undefined>();
  const models = useQuery({
    queryKey: ["compose-models"],
    queryFn: () =>
      apiFetch<{ models: components["schemas"]["AssistantModelsData"] }>(
        "/api/v1/mail/compose/models",
      ),
  });
  const defaultModel = models.data?.models?.current ?? "";

  useEffect(() => {
    setModel(undefined);
  }, [intent.key]);

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
    mutationFn: async (input: {
      action: ComposeAssistAction;
      instruction: string;
      revision?: RevisionBaseline;
      model?: string;
    }) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const live = splitTextRegion(current.bodyMarkdown);
      const sentText = input.revision?.sentText ?? live.text;
      const to = firstAddress(current.frontmatter.to);
      if (!intent.messageId && !to) throw new Error("Add a recipient before drafting");
      const subject = current.frontmatter.subject.trim();
      if (!intent.messageId && !live.text.trim() && !input.instruction.trim() && !subject) {
        throw new Error("Say what this email should do");
      }
      const range = selectedRegionRange(live, selection);
      const request: ComposeAssistRequest = {
        draft: assistDraft(intent, current),
        text: sentText,
        action: input.action,
        adjust,
      };
      if (input.model) request.model = input.model;
      const selected = input.revision?.selection;
      if (selected) request.selection = selected;
      else if (!input.revision && range) {
        request.selection = {
          start: utf8Offset(live.text, range.from),
          end: utf8Offset(live.text, range.to),
        };
      }
      const note = input.instruction.trim();
      if (note) request.instruction = note;
      if (input.revision?.proposalId) request.revise = input.revision.proposalId;
      if (!current.binding) throw new Error("Draft has no workspace owner; reopen it explicitly");
      const response = await requestComposeAssist(request, current.binding);
      return {
        response,
        sentText,
        action: input.action,
        instruction: input.instruction,
        selection: request.selection ?? undefined,
        model: input.model,
      };
    },
    onSuccess: ({
      response,
      sentText,
      action,
      instruction: asked,
      selection: selected,
      model: chosen,
    }) => {
      const answeredModel = response.proposal.provenance?.model;
      const selectedModel =
        chosen ??
        (answeredModel && models.data?.models?.choices.some((choice) => choice.id === answeredModel)
          ? answeredModel
          : undefined);
      setModel(selectedModel);
      const next: AssistVersion = {
        proposal: response.proposal,
        sentText,
        action,
        instruction: asked,
        selection: selected,
        model: selectedModel,
      };
      setProof((current) => ({
        versions: [...current.versions, next],
        index: current.versions.length,
        open: true,
      }));
    },
    onError: (error) => toast.error("Draft failed", { description: errorMessage(error) }),
  });

  function run(
    action: ComposeAssistAction,
    note: string,
    revision?: RevisionBaseline,
    modelSelection?: { model: string | undefined },
  ) {
    if (inFlight.current) return;
    inFlight.current = true;
    ask.mutate(
      { action, instruction: note, revision, model: modelSelection ? modelSelection.model : model },
      { onSettled: () => (inFlight.current = false) },
    );
  }

  function submit() {
    run(helpAction(helpStateNow()), instruction);
  }

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

  function again(note?: string) {
    if (!version) return;
    const trimmed = note?.trim() ?? "";
    if (trimmed) {
      run(
        version.action,
        trimmed,
        {
          proposalId: version.proposal.proposal_id,
          sentText: version.sentText,
          selection: version.selection,
        },
        { model: version.model },
      );
      return;
    }
    const current = draftRef.current;
    const stale = version.sentText !== (current ? splitTextRegion(current.bodyMarkdown).text : "");
    if (stale) run(helpAction(helpStateNow()), version.instruction);
    else run(version.action, version.instruction, undefined, { model: version.model });
  }

  function againWithModel(selectedModel: string | undefined) {
    if (!version) return;
    setModel(selectedModel);
    const current = draftRef.current;
    const stale = version.sentText !== (current ? splitTextRegion(current.bodyMarkdown).text : "");
    if (stale) {
      run(helpAction(helpStateNow()), version.instruction, undefined, { model: selectedModel });
      return;
    }
    run(
      version.action,
      version.instruction,
      { sentText: version.sentText, selection: version.selection },
      { model: selectedModel },
    );
  }

  function accept() {
    const current = draftRef.current;
    if (!current || !version) return;
    const live = splitTextRegion(current.bodyMarkdown);
    if (live.text !== version.sentText) return;
    const { body, textStart } = joinTextRegion(live, version.proposal.text);
    const inserted = version.proposal.text.trimEnd().length;
    const previous = current.modelSpans ?? NO_SPANS;
    const kept = keptRuns(version.proposal.changes, live.text, version.proposal.text);
    const spans = kept
      ? carryModelSpans(
          previous,
          {
            oldRegionStart: live.prefix.length,
            oldRegionEnd: live.prefix.length + live.text.length,
            newRegionStart: textStart,
            insertedLength: inserted,
            suffixShift: body.length - live.suffix.length - (live.prefix.length + live.text.length),
          },
          kept,
        )
      : mapModelSpans(previous, current.bodyMarkdown, body);
    const added = addedSpans(
      proofRuns(version.proposal.changes, version.proposal.gaps),
      textStart,
      inserted,
    );
    const modelSpans = added.length > 0 ? [...spans, ...added] : spans;
    setUndo({ before: current.bodyMarkdown, beforeSpans: current.modelSpans, after: body });
    setDraft((draft) => (draft ? { ...draft, bodyMarkdown: body, modelSpans } : draft));
    setDirty(true);
    setProof(EMPTY_PROOF);
    setEditorFocusNonce((nonce) => nonce + 1);
  }

  function keepMine() {
    setProof(EMPTY_PROOF);
    setEditorFocusNonce((nonce) => nonce + 1);
  }

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
    editorFocusNonce,
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
    againWithModel,
    generating: ask.isPending,
    model,
    setModel,
    defaultModel,
    modelChoices: models.data?.models?.choices ?? [],
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
