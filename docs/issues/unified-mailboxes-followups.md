# Legacy shared thread ids list once in merged mailboxes

**Status:** open, not fixing · **Severity:** low

Gmail thread ids synced before v0.4.52 are not account-scoped
(`ThreadId::from_provider_id`), so two accounts that saw the same Gmail thread
store one id. `Store::get_threads_batch` keys on it, so a merged "All accounts"
listing returns that thread once, under one account. Pinned by
`a_thread_id_shared_across_accounts_is_listed_once` in
`crates/store/src/system_mailbox.rs`.

Not fixed: giving one account a new id means rewriting every table that stores
a thread id, and a re-sync with scoped ids can strand to-dos, records,
commitments and links that point at the old one. The benefit is display only.
Mail synced since v0.4.52 is account-scoped and does not collide.
