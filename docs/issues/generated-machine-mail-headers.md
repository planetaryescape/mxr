# Generated machine mail carries only List-Unsubscribe

Status: deferred from R1b. A decision about classifier signals, so it is not made in the generator rung.

The generated demo's machine and list families (`crates/provider-fake/src/fixtures/scenarios/bulk/`) set no raw headers. The only automation signal they carry is `List-Unsubscribe`, on newsletters and promotions. Real transactional and bulk mail carries more: `Auto-Submitted`, `Precedence`, `List-Id`, `Feedback-ID`, `Authentication-Results`. Today's classifier reads some of these (`mail_evidence.rs`, `mail_kind.rs`), so a corpus without them tests the rules that fall back to the sender's address and body, and understates what the classifier does with real mail.

After R1b, every remaining who and home miss on the full set is "possible personal mail; no decisive list or automation evidence": the travel senders, spam and the phishing lookalike (blueprint 32, "Baseline: today's classifier on the full set").

## What to decide

- Which headers each family carries, and in what share of its phrasings. A header on every email makes the corpus easy; real senders vary.
- Whether the headers belong on the visible phrasings, the held-out ones, or both. A held-out phrasing that is the only one without the header would test the header, not the wording.
- That the change moves the baseline, so it needs its own full-set run and a reviewed snapshot diff, and the follow-up numbers in blueprint 32 and CONTRIBUTING.

Add the headers through `body.metadata.raw_headers`, as `twins.rs` does, so the facts the classifier reads are the ones it would read from a provider.
