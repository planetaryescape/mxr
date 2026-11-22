import type { components } from "@/api/generated";

type Schemas = components["schemas"];

export type ComposeAssistRequest = Schemas["ComposeAssistBody"];
export type ComposeAssistResponse = Schemas["ComposeAssistResponse"];
export type ComposeAssistProposal = Schemas["ComposeProposalData"];
export type ComposeAssistDraft = Schemas["DraftTargetData"];
export type ComposeAssistAction = Schemas["AssistActionData"];
export type ComposeAssistAdjust = Schemas["AssistAdjustData"];
export type ComposeAssistAddress = Schemas["Address"];
export type ComposeAssistChange = Schemas["TextChangeData"];
export type ComposeAssistGap = Schemas["GapData"];
