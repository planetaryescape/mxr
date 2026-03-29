# IPC Audit

This audit classifies every protocol item in [crates/protocol/src/types.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/protocol/src/types.rs) using the four-bucket model:

- `core-mail`
- `mxr-platform`
- `admin-maintenance`
- `client-specific`

Current conclusion:

- `core-mail` is the largest and most stable bucket.
- `mxr-platform` holds reusable mxr runtime features that are not timeless mail concepts.
- `admin-maintenance` remains in IPC, but is conceptually fenced off.
- No current `Request`, `ResponseData`, or `DaemonEvent` variants should live in `client-specific`.
- Client-specific shaping already lives in [crates/tui](/Users/bhekanik/code/planetaryescape/mxr/crates/tui) and [crates/web](/Users/bhekanik/code/planetaryescape/mxr/crates/web).

## Requests

| Item | Kind | Category | Current owner | Correct owner | Action | Rationale | Follow-up notes |
|---|---|---|---|---|---|---|---|
| `ListEnvelopes` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Reusable mailbox listing. | Stable mail read surface. |
| `ListEnvelopesByIds` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Reusable fetch-by-id primitive. | Used by web to shape views client-side. |
| `GetEnvelope` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Canonical envelope read. | Stable. |
| `GetBody` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Canonical body read. | Stable. |
| `GetHtmlImageAssets` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Mail rendering asset resolution, not screen layout. | Reusable by multiple clients. |
| `DownloadAttachment` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Attachment materialization is a reusable mail workflow. | Stable. |
| `OpenAttachment` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Attachment open/materialize workflow. | CLI/web/TUI can all use it. |
| `ListBodies` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Batch body fetch is reusable mail data access. | Used by web thread shaping. |
| `GetThread` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Core mail thread read. | Stable. |
| `ListLabels` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Internal label/folder model exposure. | Shared by clients. |
| `CreateLabel` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Mail label mutation. | Not a screen concern. |
| `DeleteLabel` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Mail label mutation. | Stable. |
| `RenameLabel` | Request | core-mail | `handler::mailbox` | `handler::mailbox` | keep | Mail label mutation. | Stable. |
| `ListAccounts` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Runtime account inventory is an mxr product capability. | Shared across CLI/TUI/web. |
| `ListAccountsConfig` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Config-backed account definitions are mxr runtime config. | Not timeless mail protocol. |
| `AuthorizeAccountConfig` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Account authorization flow is mxr platform/runtime. | Stable platform surface. |
| `UpsertAccountConfig` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Account config mutation is mxr platform/runtime. | Shared across clients. |
| `SetDefaultAccount` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Default sender/runtime account is mxr-level behavior. | Platform setting. |
| `TestAccountConfig` | Request | mxr-platform | `handler::accounts` | `handler::accounts` | keep | Account test flow is product/runtime capability. | Keep in daemon IPC. |
| `ListRules` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Rules are mxr product logic, not raw mail semantics. | Shared capability. |
| `GetRule` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Rule inspection is product/runtime. | Stable platform surface. |
| `GetRuleForm` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Editable rule form is product/runtime, not screen state. | Keep but document as platform-specific. |
| `UpsertRule` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Rules management belongs to mxr platform. | Stable. |
| `UpsertRuleForm` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Form-oriented rule upsert is product/runtime convenience shared by clients. | Keep, do not treat as TUI-only. |
| `DeleteRule` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Rules management belongs to mxr platform. | Stable. |
| `DryRunRules` | Request | mxr-platform | `handler::rules` | `handler::rules` | keep | Deterministic preview is a core mxr rules capability. | Platform, not admin. |
| `ListSavedSearches` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Saved searches are mxr product primitives. | Rehomed from diagnostics bucket. |
| `ListSubscriptions` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Subscription summaries are reusable mxr product views over mail data. | Shared by web/TUI/CLI. |
| `GetSemanticStatus` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Semantic runtime state is mxr product/runtime capability. | Not mere maintenance. |
| `EnableSemantic` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Semantic enablement is mxr runtime config. | Platform-owned. |
| `InstallSemanticProfile` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Profile install is mxr runtime feature work. | Platform-owned. |
| `UseSemanticProfile` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Active profile selection is mxr runtime config. | Platform-owned. |
| `ReindexSemantic` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Reindex here is tied to semantic runtime feature, not generic daemon health. | Still operationally heavy; documented as platform/runtime. |
| `CreateSavedSearch` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Saved search creation is a product primitive. | Rehomed from diagnostics. |
| `DeleteSavedSearch` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Saved search management is product/runtime. | Rehomed from diagnostics. |
| `RunSavedSearch` | Request | mxr-platform | `handler::diagnostics` | `handler::platform` | move | Running a saved search is a product/runtime capability. | Returns core mail search results. |
| `ListEvents` | Request | admin-maintenance | `handler::diagnostics` | `handler::admin` | move | Operational event inspection. | Keep in IPC, fence conceptually. |
| `GetLogs` | Request | admin-maintenance | `handler::diagnostics` | `handler::admin` | move | Operational log inspection. | Not core mail API. |
| `GetDoctorReport` | Request | admin-maintenance | `handler::diagnostics` | `handler::admin` | move | Runtime health/repair inspection. | Keep in IPC. |
| `GenerateBugReport` | Request | admin-maintenance | `handler::diagnostics` | `handler::admin` | move | Operational support/diagnostic workflow. | Keep in IPC. |
| `Search` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Search is core mail navigation/data access. | Should not live in diagnostics. |
| `SyncNow` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Mail sync is a core runtime workflow. | Stable mail/runtime surface. |
| `GetSyncStatus` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Per-account sync state supports mail runtime workflows. | Not admin-only. |
| `SetFlags` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Mail mutation. | Stable. |
| `Count` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Search count is core mail navigation. | Should not live in diagnostics. |
| `GetHeaders` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Raw headers are reusable mail data. | Stable read surface. |
| `ListRuleHistory` | Request | core-mail | `handler::rules` | `handler::rules` | keep | Historical executions are mail-affecting workflow history. | Kept in rules module; conceptually adjacent to mail ops. |
| `Mutation` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Bulk/single message mutations are durable mail verbs. | Nested commands stay mail bucket. |
| `Unsubscribe` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Mail action based on message metadata. | Stable workflow. |
| `Snooze` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Mail lifecycle workflow. | Stable. |
| `Unsnooze` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Mail lifecycle workflow. | Stable. |
| `ListSnoozed` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Mail state listing. | Stable. |
| `PrepareReply` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Reusable compose workflow. | Stable mail/runtime. |
| `PrepareForward` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Reusable compose workflow. | Stable mail/runtime. |
| `SendDraft` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Core send workflow. | Stable. |
| `SaveDraftToServer` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Core draft workflow. | Stable. |
| `ListDrafts` | Request | core-mail | `handler::mutations` | `handler::mutations` | keep | Core draft listing. | Stable. |
| `ExportThread` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Export is reusable mail data workflow. | Not admin. |
| `ExportSearch` | Request | core-mail | `handler::diagnostics` | `handler::runtime` | move | Export is reusable mail data workflow. | Not admin. |
| `GetStatus` | Request | admin-maintenance | `handler::diagnostics` | `handler::admin` | move | Daemon status is operational inspection, not mail domain. | Kept in IPC, fenced off. |
| `Ping` | Request | admin-maintenance | `protocol utility` | `handler::admin` | document | Transport/daemon liveness probe. | Operational utility. |
| `Shutdown` | Request | admin-maintenance | `server control` | `handler::admin` | document | Daemon lifecycle control. | High-friction admin surface. |

## ResponseData

| Item | Kind | Category | Current owner | Correct owner | Action | Rationale | Follow-up notes |
|---|---|---|---|---|---|---|---|
| `Envelopes` | ResponseData | core-mail | mailbox/runtime handlers | mailbox/runtime handlers | keep | Reusable envelope payload. | Client groups it locally. |
| `Envelope` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Canonical envelope read payload. | Stable. |
| `Body` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Canonical body payload. | Stable. |
| `HtmlImageAssets` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Reusable rendering asset metadata. | Not a screen payload. |
| `AttachmentFile` | ResponseData | core-mail | mailbox/export handlers | mailbox/export handlers | keep | Reusable attachment materialization result. | Stable. |
| `Bodies` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Batch body payload. | Shared by web/TUI. |
| `Thread` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Canonical thread payload. | Web shapes right rail itself. |
| `Labels` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Canonical label/folder payload. | Stable. |
| `Label` | ResponseData | core-mail | mailbox handlers | mailbox handlers | keep | Single label mutation result. | Stable. |
| `SearchResults` | ResponseData | core-mail | runtime/platform handlers | runtime/platform handlers | keep | Reusable mail search result payload. | Returned by both raw and saved search flows. |
| `SyncStatus` | ResponseData | core-mail | runtime handlers | runtime handlers | keep | Per-account mail runtime state. | Stable. |
| `Count` | ResponseData | core-mail | runtime handlers | runtime handlers | keep | Reusable search count payload. | Stable. |
| `Headers` | ResponseData | core-mail | runtime handlers | runtime handlers | keep | Reusable raw mail header payload. | Stable. |
| `ReplyContext` | ResponseData | core-mail | mutation handlers | mutation handlers | keep | Reusable compose context. | Stable. |
| `ForwardContext` | ResponseData | core-mail | mutation handlers | mutation handlers | keep | Reusable compose context. | Stable. |
| `Drafts` | ResponseData | core-mail | mutation handlers | mutation handlers | keep | Reusable draft listing. | Stable. |
| `SnoozedMessages` | ResponseData | core-mail | mutation handlers | mutation handlers | keep | Reusable snooze listing. | Stable. |
| `ExportResult` | ResponseData | core-mail | runtime/export handlers | runtime/export handlers | keep | Export output is a reusable mail workflow result. | Stable. |
| `Rules` | ResponseData | mxr-platform | rules handlers | rules handlers | keep | Rules are mxr product/runtime entities. | Platform-owned. |
| `RuleData` | ResponseData | mxr-platform | rules handlers | rules handlers | keep | Rule payload is product/runtime config. | Platform-owned. |
| `Accounts` | ResponseData | mxr-platform | accounts handlers | accounts handlers | keep | Runtime account inventory is platform state. | Stable. |
| `AccountsConfig` | ResponseData | mxr-platform | accounts handlers | accounts handlers | keep | Config-backed account definitions are platform state. | Stable. |
| `AccountOperation` | ResponseData | mxr-platform | accounts handlers | accounts handlers | keep | Account setup/test/auth result is platform workflow output. | Stable. |
| `RuleFormData` | ResponseData | mxr-platform | rules handlers | rules handlers | keep | Rule editing payload is product/runtime-facing. | Keep but do not expand into screen state. |
| `RuleDryRun` | ResponseData | mxr-platform | rules handlers | rules handlers | keep | Deterministic rule preview is platform behavior. | Stable. |
| `SavedSearches` | ResponseData | mxr-platform | diagnostics/runtime handlers | platform handlers | move | Saved searches are mxr product primitives. | Rehomed. |
| `Subscriptions` | ResponseData | mxr-platform | diagnostics/runtime handlers | platform handlers | move | Subscription summaries are mxr-level runtime capability. | Rehomed. |
| `SemanticStatus` | ResponseData | mxr-platform | diagnostics/runtime handlers | platform handlers | move | Semantic runtime state is product/runtime capability. | Rehomed. |
| `SavedSearchData` | ResponseData | mxr-platform | diagnostics/runtime handlers | platform handlers | move | Saved search create/update result is platform data. | Rehomed. |
| `EventLogEntries` | ResponseData | admin-maintenance | diagnostics handlers | admin handlers | move | Operational event history. | Fence off conceptually. |
| `LogLines` | ResponseData | admin-maintenance | diagnostics handlers | admin handlers | move | Operational log payload. | Fence off conceptually. |
| `DoctorReport` | ResponseData | admin-maintenance | diagnostics handlers | admin handlers | move | Repair/health/report payload. | Fence off conceptually. |
| `BugReport` | ResponseData | admin-maintenance | diagnostics handlers | admin handlers | move | Support/diagnostic output. | Fence off conceptually. |
| `RuleHistory` | ResponseData | admin-maintenance | rules handlers | rules/admin boundary | document | Historical operational output about rule runs. | Kept where it is for now; conceptual overlap documented. |
| `Status` | ResponseData | admin-maintenance | diagnostics handlers | admin handlers | move | Daemon/runtime health snapshot. | Not core mail API. |
| `Pong` | ResponseData | admin-maintenance | protocol utility | admin utility | document | Liveness probe response. | Operational utility. |
| `Ack` | ResponseData | admin-maintenance | protocol utility | protocol utility | document | Cross-cutting generic success marker. | Transitional utility reused by multiple buckets. |

## DaemonEvent

| Item | Kind | Category | Current owner | Correct owner | Action | Rationale | Follow-up notes |
|---|---|---|---|---|---|---|---|
| `SyncCompleted` | DaemonEvent | core-mail | daemon loops | daemon runtime | keep | Mail runtime event shared by clients. | Stable. |
| `SyncError` | DaemonEvent | core-mail | daemon loops | daemon runtime | keep | Mail runtime error event shared by clients. | Stable. |
| `NewMessages` | DaemonEvent | core-mail | protocol/loops | daemon runtime | deprecate | Legitimate mail event, but currently unused in emitted paths and clients. | Keep wire compatibility for now; do not expand until needed. |
| `MessageUnsnoozed` | DaemonEvent | core-mail | daemon loops | daemon runtime | keep | Mail lifecycle event. | Stable. |
| `LabelCountsUpdated` | DaemonEvent | core-mail | daemon loops | daemon runtime | keep | Reusable mail metadata event. | TUI uses it without daemon screen shaping. |

## Client-specific boundary

No current protocol variants were classified as `client-specific`.

That is the right outcome.

Current client-owned shaping already lives in:

- [crates/tui/src/local_state.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/tui/src/local_state.rs)
- [crates/tui/src/daemon_events.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/tui/src/daemon_events.rs)
- [crates/web/src/chrome.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/web/src/chrome.rs)
- [crates/web/src/envelope_list.rs](/Users/bhekanik/code/planetaryescape/mxr/crates/web/src/envelope_list.rs)

Examples kept out of daemon IPC:

- TUI pane/tab/selection/sidebar state
- Web shell/sidebar JSON
- Web date-bucket grouping
- Web right-rail/thread presentation payloads

## Transitional compromises

- `Ack` remains a cross-cutting utility response rather than being split per bucket.
- `RuleHistory` still sits near rules semantics in code, even though it also has operational flavor.
- `NewMessages` is kept for compatibility but should be treated as dormant until a real multi-client use appears.
