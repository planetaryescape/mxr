# Three small corrections follow a review with no blockers

Claude Opus 5.5/max reviewed `010ae9518c5ee9bf96c194628483e10a0f0ac4ce`,
tree `c04fbff55205d5f8bcb8fa9048bb8f07953e4642`, and returned GREEN with six
should-fix findings. [The report](s02-claude-010ae951-pinned.md) records its
limits; [session metadata](s02-claude-010ae951-pinned-metadata.json) confirms
the model, effort and 92 read/search calls. Review took 20m56.479s.

Root fetched main `6ed0554c` and read the cited source and relevant tests at
the exact candidate on 11 October 2026. The disposition is based on the code
and scope, rather than the reviewer's severity labels.

| Finding | Disposition and evidence |
| --- | --- |
| Reopened alias draft displays the account primary address | Adopt a small correction. `crates/web/src/lib.rs:2986` reconstructs frontmatter with `account.email`; `crates/compose/src/draft_codec.rs:59` already preserves `draft.from`. Reuse that helper and verify the restored sender. This keeps the displayed sender aligned with the stored draft. |
| Typing during explicit Retry retains the older recovery token | Defer UX improvement. `useDraftSaveActions.ts:104-121` refuses to adopt a response after newer typing. `useRecoveryAttachments.test.tsx:350` explicitly proves newer text and owned attachment paths remain on the private recovery. This can require opening canonical content and copying the newer text; it does not silently lose or mark that text saved. |
| Provider-linked drafts contact the provider on autosave | Record the current limit. `crates/web/src/lib.rs:2714` uses `UpdateDraft`; `crates/daemon/src/handler/mutations.rs:2519-2563` updates a linked provider draft before local canonical content. Failure preserves private recovery through `lib.rs:910-925`. Decoupling local saves requires reconciliation to track unsynced content, or later provider pulls could overwrite it. It is not a safe one-line bypass. |
| Discard fails after the file or canonical draft was removed elsewhere | Defer idempotency improvement. `lib.rs:158-172` requires a canonicalized existing file; `lib.rs:1402-1419` propagates the canonical delete error before file removal. The private work remains and Close remains available. S02's verified recovery-only Discard journey passes. |
| Orphaned-draft output advertises immediate deletion | Adopt a wording correction. `commands/mutations/compose.rs:866` advertises delete, while the revision/status gate refuses Sending rows. Point to inspection/resolution before deletion without promising that another send is safe. S03 owns uncertain outcomes. |
| Reopening during transport failure forgets recovery | Adopt a narrow correction. `session/activeDrafts.ts:31-57` catches every error and removes the remembered path. `api/client.ts:55,88-118` already distinguishes `DaemonUnavailableError` from a missing-file response. Rethrow that existing error before forgetting the entry; test reconnect loads the same private text/path. Preserve the existing missing-file fallback. |

Final candidate `3d338bebfa9e6b714b6f9503ed5546c037d5eef1`, tree
`3a903ddea1843b4c380cd81adf344882e7d39c42`, reuses the shared frontmatter
helper, CLI wording, existing transport-error guard and focused regression tests.
Root reviewed the five-file delta; its commit range and source hash are in the [reproducible proof](s02-trivial-delta-3d338beb-proof.json).
A test-only follow-up replaced conditional assertions to satisfy existing lint. The orchestrate skill's
trivial post-triage exception applies: root inspected the exact delta against
`010ae951`; changed checks and CI on the resulting head remain required. No new
protocol, provider ordering or accepted-retry ownership design belongs in this
correction. A broader delta would invalidate that exception.

[Draft recovery follow-ups](../../../issues/draft-recovery-followups.md) retain
the remaining verified limitations. The reviewer also listed diagnostic/schema
presentation nits and an attachment-copy hypothesis. Those do not change this
release's acceptance; no untested failure is claimed reproduced.
