import type { components } from "../../../web/src/api/generated";

export type Schemas = components["schemas"];
type ResponseData = Schemas["ResponseData"];
export type Reply<K extends ResponseData["kind"]> = Extract<ResponseData, { kind: K }>;

export type NowData = Schemas["NowData"];
export type MessagesData = Schemas["MessagesData"];
export type MessagesRow = Schemas["MessagesRowData"];
export type TodoRunway = Schemas["TodoRunwayData"];
export type Todo = Schemas["TodoData"];
export type UpdatesDigest = Schemas["UpdatesDigestData"];
export type UpdateLine = Schemas["UpdateLineData"];
export type ReadingEdition = Schemas["ReadingEditionData"];
export type ReadingItem = Schemas["ReadingItemData"];
export type RecordLedger = Schemas["RecordLedgerData"];
export type RecordRow = Schemas["RecordData"];
export type DeskRow = Schemas["DeskRowData"];
export type ModeKind = Schemas["ModeKindData"];
export type ComposeSession = Schemas["ComposeSessionData"];
export type ComposeAssistRequest = Schemas["ComposeAssistBody"];
export type ComposeModelChoice = Schemas["ModelChoiceData"];
export type ComposeModelCatalogue = Schemas["AssistantModelsData"];
export type ComposeAssistProposal = Schemas["ComposeProposalData"];
export type ComposeAssistAction = Schemas["AssistActionData"];
export type SafetyReport = Schemas["DraftSafetyReport"];
export type Workspace = Schemas["ArchiveWorkspaceData"];
