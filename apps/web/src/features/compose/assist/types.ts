/*
 * The compose assist contract as the web app sees it: what "Draft for me"
 * sends and what comes back as a proof. Mirrors the Rust shapes in
 * crates/protocol/src/types/compose_assist.rs (commit 3635c6e64); the bridge
 * route that carries them is not in the tree yet, so no generated type exists
 * for them. Offsets are UTF-8 bytes into the text region (the body without
 * its signature block and forward header).
 */

import type { DraftProvenance } from "../draftProvenanceFormat";

export interface ComposeAssistAddress {
  name: string | null;
  email: string;
}

export type ComposeAssistDraft =
  | { kind: "stored"; draft_id: string }
  | { kind: "reply"; message_id: string; all: boolean }
  | { kind: "forward"; message_id: string; to: ComposeAssistAddress[] }
  | { kind: "new"; account_id: string; to: ComposeAssistAddress[]; subject: string };

export type ComposeAssistAction = "write" | "finish" | "polish" | "edit_selection";

export interface ComposeAssistSelection {
  start: number;
  end: number;
}

export interface ComposeAssistAdjust {
  shorter: boolean;
  warmer: boolean;
  more_formal: boolean;
  proofread: boolean;
}

export interface ComposeAssistRequest {
  draft: ComposeAssistDraft;
  text: string;
  selection?: ComposeAssistSelection;
  action?: ComposeAssistAction;
  instruction?: string;
  adjust: ComposeAssistAdjust;
}

export type ComposeAssistChangeKind = "kept" | "added" | "removed";

export interface ComposeAssistChange {
  kind: ComposeAssistChangeKind;
  text: string;
}

export interface ComposeAssistGap {
  label: string;
  start: number;
  end: number;
}

export interface ComposeAssistProposal {
  proposal_id: string;
  action: ComposeAssistAction;
  /** Hex SHA-256 of the request text the proof was written against. */
  base: string;
  /** The full text region the proof proposes. */
  text: string;
  changes: ComposeAssistChange[];
  gaps: ComposeAssistGap[];
  provenance: DraftProvenance | null;
  yours_changed: number[];
  composer_version: string;
}

export interface ComposeAssistResponse {
  proposal: ComposeAssistProposal;
}
