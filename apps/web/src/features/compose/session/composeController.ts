/*
 * The contract between the compose session hook and every compose surface:
 * what the UI reads and the handlers it calls.
 */

import type { Dispatch, KeyboardEvent, RefObject, SetStateAction } from "react";

import type {
  ComposeFrontmatter,
  ComposeIssue,
  DraftSafetyReport,
  RuntimeAccount,
  SuggestedCollaborator,
} from "../api";
import type {
  DraftLengthHint,
  DraftRefineKnobs,
  DraftSuggestionResponse,
  VoiceRegister,
} from "../types";
import type { ComposeDraftState, ComposeIntent, Snippet } from "./composeDraft";
import type { ComposeUploadProgress } from "./useComposeAttachments";

export interface Signature {
  id: string;
  name: string;
  body: string;
}

/** Everything the compose UI consumes from the session lifecycle. */
export interface ComposeController {
  intent: ComposeIntent;
  sessionLoading: boolean;
  sessionError: Error | null;
  retrySession: () => void;

  draft: ComposeDraftState | null;
  dirty: boolean;
  saveStatus: string;
  saveError: string | null;
  /** Validation issues worth showing now: none before the first send
   * attempt, except malformed addresses once a recipient field is left. */
  visibleIssues: ComposeIssue[];
  markRecipientsTouched: () => void;
  recipientCount: number;
  runtimeAccounts: RuntimeAccount[];
  selectedAccount: RuntimeAccount | undefined;
  /** Send-as addresses (primary + aliases) for the selected account. */
  accountAddresses: string[];
  canServerSave: boolean;
  busy: boolean;
  uploading: number;
  sending: boolean;
  discarding: boolean;

  showCc: boolean;
  setShowCc: Dispatch<SetStateAction<boolean>>;
  showBcc: boolean;
  setShowBcc: Dispatch<SetStateAction<boolean>>;
  revealCc: () => void;
  revealBcc: () => void;

  toInputRef: RefObject<HTMLInputElement | null>;
  ccInputRef: RefObject<HTMLInputElement | null>;
  bccInputRef: RefObject<HTMLInputElement | null>;
  fileInputRef: RefObject<HTMLInputElement | null>;

  sendConfirmOpen: boolean;
  setSendConfirmOpen: Dispatch<SetStateAction<boolean>>;
  discardConfirmOpen: boolean;
  setDiscardConfirmOpen: Dispatch<SetStateAction<boolean>>;

  updateFrontmatter: <K extends keyof ComposeFrontmatter>(
    field: K,
    value: ComposeFrontmatter[K],
  ) => void;
  updateBody: (value: string) => void;
  updateAccount: (accountId: string) => void;
  handleSaveClick: () => Promise<void>;
  handleServerSaveClick: () => Promise<void>;
  handleRefreshClick: () => Promise<void>;
  handleAttachShortcut: () => void;
  handleComposeKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  requestSend: () => void;
  /** Send from the confirm dialog; `override` only after an explicit
   * override of a blocked safety report. */
  confirmSend: (override: boolean) => Promise<void>;
  snippetPickerOpen: boolean;
  setSnippetPickerOpen: Dispatch<SetStateAction<boolean>>;
  /** Snippets available for the picker and `;name ` inline expansion. */
  snippetList: Snippet[];
  /** Append a snippet body to the end of the message body. */
  insertSnippet: (body: string) => void;
  signaturePickerOpen: boolean;
  setSignaturePickerOpen: Dispatch<SetStateAction<boolean>>;
  signatureList: Signature[];
  /** Append a signature block (`\n\n--\n{body}`) to the message body. */
  insertSignature: (body: string) => void;
  /** "Maybe include" suggestions for the chip row; empty when none or the
   * lookup failed (the row hides silently). */
  collaboratorSuggestions: SuggestedCollaborator[];
  /** Append an address to Cc (suggested-collaborator click), revealing Cc. */
  addCc: (email: string) => void;
  sendLaterOpen: boolean;
  setSendLaterOpen: Dispatch<SetStateAction<boolean>>;
  /** Custom-time dialog for "Send and remind me if no reply in...". */
  remindDialogOpen: boolean;
  setRemindDialogOpen: Dispatch<SetStateAction<boolean>>;
  /** Send, then set a no-reply reminder for `at`. */
  requestSendAndRemind: (at: Date, label: string) => void;
  /** Send, then archive the source conversation (replies only). */
  requestSendAndArchive: () => void;
  /** Open the send-later dialog (same local validation gate as send). */
  requestSendLater: () => void;
  /** Persist the session as a stored draft and schedule it for `at`. */
  scheduleSend: (at: Date, label?: string) => Promise<void>;
  scheduling: boolean;
  /** Pre-send safety report backing the confirm dialog; null when the
   * check passed clean (no dialog) or hasn't run. */
  safetyReport: DraftSafetyReport | null;
  /** Set when the safety check itself failed — dialog shows a notice. */
  safetyCheckError: string | null;
  checkingSafety: boolean;
  requestDiscard: () => void;
  discardDraft: () => Promise<void>;
  /** Save pending edits, then call `options.onClose`. Stays open (with a
   * toast) when the save fails, so closing never drops text. */
  requestClose: () => Promise<void>;
  retrySave: () => void;
  addFiles: (files: FileList | File[]) => Promise<void>;
  /** In-flight upload entries for the attachments strip (cleared when the
   * batch settles). */
  uploadProgress: ComposeUploadProgress[];
  removeAttachment: (path: string) => void;

  assistOpen: boolean;
  setAssistOpen: Dispatch<SetStateAction<boolean>>;
  aiPurpose: string;
  setAiPurpose: Dispatch<SetStateAction<string>>;
  aiRegister: VoiceRegister;
  onRegisterChange: (value: VoiceRegister) => void;
  aiLength: DraftLengthHint;
  onLengthChange: (value: DraftLengthHint) => void;
  aiOverridden: boolean;
  resetTone: () => void;
  refineContext: string;
  setRefineContext: Dispatch<SetStateAction<string>>;
  draftSuggestion: DraftSuggestionResponse | null;
  generateDraft: () => void;
  generating: boolean;
  runRefine: (knobs: DraftRefineKnobs) => void;
  refining: boolean;
  canRefine: boolean;
}
