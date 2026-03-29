# mxr — Internal Model Audit

## Judgment

The current internal model is broadly honest and durable. It does not need a rewrite.

The strongest parts are already the right parts:
- split `MailSyncProvider` / `MailSendProvider`
- provider-scoped IDs, kinds, cursors, and capabilities
- native thread IDs when available, JWZ-style reconstruction when not
- local-first drafts with `reply_headers` as the reply/threading surface

The main pressure point is labels vs folders. The current abstraction is still acceptable, but only if contributors keep the seam explicit instead of flattening IMAP into fake Gmail labels.

## Keep

- One unified app/runtime mail model.
- Provider truth preserved through `ProviderKind`, `provider_id`, `SyncCursor`, and `SyncCapabilities`.
- `LabelKind::Folder` as the app-visible marker for folder-backed placement.
- `native_thread_ids` + JWZ/subject fallback threading split.
- Local-first draft model. Server drafts stay optional provider capability.

## Tighten

- Folder-backed mutations must not use Gmail-style optimistic local label union/removal.
- For providers with `SyncCapabilities.labels == false`, folder-affecting mutations should reconcile through provider sync so store/search reflect provider truth.
- Snooze on folder-backed providers must not assume the pre-move `MessageId` survived. This pass re-anchors snooze state to the reconciled post-sync message copy.

## Document

- `server_search` is provider truth, not a promise that the app always routes search there.
- `ProviderMeta` is reserved/dormant in the current implementation, not live sync truth.
- `Envelope.provider_id` is provider-instance identity, not a universal logical-message identity.
- IMAP moves/copies may materialize as delete+create.

## Area Audit

| Area | Judgment | Notes |
|---|---|---|
| Labels vs folders | Tighten | Unified organizer surface is fine. Honesty seam is `LabelKind::Folder` + `SyncCapabilities.labels == false`. Do not pretend folder placement is stable multi-label state. |
| Threading | Keep | Current native-thread-or-JWZ split is strong. Do not collapse it into one fake thread story. |
| Drafts and send semantics | Keep + document | `Draft` is local-first canonical compose state. `reply_headers` is the reply/threading surface. Server drafts are optional send-provider capability. |
| Search capability differences | Document | Keep capability visibility. `server_search` is provider truth, not app-routing policy. |
| Message identity / provider identity | Document | Gmail IDs are stable. IMAP identity is mailbox-instance-based today. Moves/copies may change `provider_id` and therefore `MessageId`. |
| Mutation semantics | Tighten | Gmail-like optimistic local label persistence is valid only for providers with real multi-assign labels. Folder providers must reconcile via sync for placement changes. |
| Sync cursor / sync capability representation | Keep | Rich provider-specific cursor/capability state is a feature, not abstraction failure. |
| Provider truth hidden too aggressively | Document | The main overstatement was docs around live `ProviderMeta` use. Live truth is already carried elsewhere. |
| Provider detail leaking upward unnecessarily | Keep | Provider kinds, IDs, capabilities, and cursors are intentional leaks. They prevent dishonest flattening. |

## Contributor Guardrails

- Do not “simplify” IMAP folders into Gmail-style labels.
- Do not delete capability flags because “the app can just branch somewhere else.”
- Do not replace native-thread IDs + fallback reconstruction with one lowest-common-denominator threading path.
- Do not describe `ProviderMeta` as active runtime truth unless code starts using it again.
- Prefer explicit provider truth over convenient lies.
