/*
 * The compose assist contract, as the generated bridge schema names it. The
 * daemon's shapes are in crates/protocol/src/types/compose_assist.rs; the
 * bridge routes are in crates/web/src/compose_assist_routes.rs. Offsets are
 * UTF-8 bytes into the text region (the body without its signature block and
 * forward header).
 */

import type { components } from "@/api/generated";

type Schemas = components["schemas"];

/** `POST /api/v1/mail/compose/assist` body. */
export type ComposeAssistRequest = Schemas["ComposeAssistBody"];
/** `POST /api/v1/mail/compose/assist` answer. */
export type ComposeAssistResponse = Schemas["ComposeAssistResponse"];
export type ComposeAssistProposal = Schemas["ComposeProposalData"];
export type ComposeAssistDraft = Schemas["DraftTargetData"];
export type ComposeAssistAction = Schemas["AssistActionData"];
export type ComposeAssistAdjust = Schemas["AssistAdjustData"];
export type ComposeAssistAddress = Schemas["Address"];
export type ComposeAssistChange = Schemas["TextChangeData"];
export type ComposeAssistGap = Schemas["GapData"];
