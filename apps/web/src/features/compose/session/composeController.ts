import type { Dispatch, KeyboardEvent, RefObject, SetStateAction } from "react";

import type {
  ComposeFrontmatter,
  ComposeIssue,
  ComposeSessionResponse,
  DraftSafetyReport,
  RuntimeAccount,
  SuggestedCollaborator,
} from "../api";
import type { ComposeDraftState, ComposeIntent, Snippet } from "./composeDraft";
import type { useDraftAssist } from "./useDraftAssist";
import type { ComposeUploadProgress } from "./useComposeAttachments";

export interface Signature {
  id: string;
  name: string;
  body: string;
}

export type DraftAssistController = ReturnType<typeof useDraftAssist>;

export interface ComposeController extends DraftAssistController {
  intent: ComposeIntent;
  sessionLoading: boolean;
  sessionError: Error | null;
  retrySession: () => void;
  reopenSavedDraft: (() => void) | null;

  draft: ComposeDraftState | null;
  dirty: boolean;
  saveStatus: string;
  saveError: string | null;
  visibleIssues: ComposeIssue[];
  markRecipientsTouched: () => void;
  recipientCount: number;
  runtimeAccounts: RuntimeAccount[];
  selectedAccount: RuntimeAccount | undefined;
  accountAddresses: string[];
  canServerSave: boolean;
  busy: boolean;
  uploading: number;
  sending: boolean;
  discarding: boolean;
  changingAudience: boolean;
  audiencePreview: ComposeSessionResponse | null;
  requestReplyAudience: (replyAll: boolean) => Promise<void>;
  confirmReplyAudience: () => Promise<void>;
  cancelReplyAudience: () => void;

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
  handleRecoveryClick: () => Promise<void>;
  handleAttachShortcut: () => void;
  handleComposeKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  requestSend: () => void;
  confirmSend: (override: boolean) => Promise<void>;
  snippetPickerOpen: boolean;
  setSnippetPickerOpen: Dispatch<SetStateAction<boolean>>;
  snippetList: Snippet[];
  insertSnippet: (body: string) => void;
  signaturePickerOpen: boolean;
  setSignaturePickerOpen: Dispatch<SetStateAction<boolean>>;
  signatureFoldOpen: boolean;
  setSignatureFoldOpen: Dispatch<SetStateAction<boolean>>;
  signatureList: Signature[];
  insertSignature: (body: string) => void;
  collaboratorSuggestions: SuggestedCollaborator[];
  addCc: (email: string) => void;
  sendLaterOpen: boolean;
  setSendLaterOpen: Dispatch<SetStateAction<boolean>>;
  remindDialogOpen: boolean;
  setRemindDialogOpen: Dispatch<SetStateAction<boolean>>;
  requestSendAndRemind: (at: Date, label: string) => void;
  requestSendAndArchive: () => void;
  requestSendLater: () => void;
  scheduleSend: (at: Date, label?: string) => Promise<void>;
  scheduling: boolean;
  safetyReport: DraftSafetyReport | null;
  safetyCheckError: string | null;
  checkingSafety: boolean;
  requestDiscard: () => void;
  discardDraft: () => Promise<void>;
  requestClose: () => Promise<void>;
  retrySave: () => void;
  addFiles: (files: FileList | File[]) => Promise<void>;
  uploadProgress: ComposeUploadProgress[];
  removeAttachment: (path: string) => void;
}
