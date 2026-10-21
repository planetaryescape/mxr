// The same generated contract the web app uses, so both clients move with
// the daemon's OpenAPI spec. Type-only: nothing from the web app is bundled.
import type { components } from "../../../web/src/api/generated";

export type Schemas = components["schemas"];
type ResponseData = Schemas["ResponseData"];
/** One `ResponseData` variant by its `kind` tag. */
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
export type SafetyReport = Schemas["DraftSafetyReport"];
export type Workspace = Schemas["ArchiveWorkspaceData"];
