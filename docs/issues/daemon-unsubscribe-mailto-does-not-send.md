# Unsubscribe by mailto only tells the user the address

**Status:** open · **Found:** delete-comment sweep, `b-daemon-rest` · **Severity:** nice-to-have

## What happens

`crates/daemon/src/unsubscribe.rs`, `UnsubscribeMethod::Mailto` arm: the daemon returns
`UnsubscribeResult::Success("Send an email to <address> to unsubscribe.")`. It does not send the
mail. A comment there said this was a stand-in for a full implementation.

## Smallest fix

Send the unsubscribe mail through `MailSendProvider` behind the existing dry-run and preview path,
or rename the result so it does not claim success when nothing was sent.
