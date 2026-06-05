/*
 * Shared compose AI types — used by ComposeRoute and the extracted
 * DraftAssist / ComposeActionBar / DraftQualityBadges components. Kept in a
 * leaf module so those components don't import back into ComposeRoute.
 */

export type VoiceRegister = "casual" | "neutral" | "formal";
export type DraftLengthHint = "short" | "medium" | "long";

export interface VoiceMatchReport {
  score: number;
  confidence: string;
  notable_deltas: string[];
}

export interface HumanizerReport {
  score: number;
  hits: { category: string; matched: string; suggestion?: string | null }[];
}

export interface DraftSuggestionResponse {
  kind: "DraftSuggestion";
  body: string;
  model: string;
  voice_match?: VoiceMatchReport | null;
  humanizer?: HumanizerReport | null;
  rewrite_iterations: number;
  /** Tone/length the daemon inferred from the relationship (effective values). */
  inferred_register?: VoiceRegister | null;
  inferred_length?: DraftLengthHint | null;
  /** Human-readable note, e.g. "Matched to alice@x (casual, short)". */
  context_note?: string | null;
}

export interface DraftRefineKnobs {
  shorter?: boolean;
  warmer?: boolean;
  more_formal?: boolean;
  less_emoji?: boolean;
  add_context?: string;
}
