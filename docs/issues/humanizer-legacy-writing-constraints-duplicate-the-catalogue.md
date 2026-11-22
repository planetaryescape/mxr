# Humanizer keeps an unused hand-written constraints string beside the catalogue

Status: open.

`writing_constraints` in `crates/humanizer/src/lib.rs` returns a fixed prompt
string. No code outside the file calls it (checked with `grep -rn
writing_constraints crates apps` at the sweep base). `email_writing_constraints`
builds the same guidance from the catalogue in
`crates/humanizer/src/catalogue.rs`, and the daemon uses that one
(`crates/daemon/src/handler/draft_refine.rs`). The two can drift.

The smallest fix: delete `writing_constraints`. `pattern_for_category` stays
while `crates/composer` calls it.
