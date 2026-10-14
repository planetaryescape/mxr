# Updates gists follow-ups

Left open by the Updates gist change (one-line gists for subject-only lines,
the number check, the folded Needs a look lines).

- **The number check drops some honest gists.** It requires every digit
  figure in a gist to be written in the mail's subjects or text. A gist that
  says "1 failed payment" when the mail says "a failed payment", or that
  takes a date from the message's `Date` header instead of its text, is
  dropped and the line shows the rule's fact. Spelled counts are checked
  (`two` against `2`), but `one`, `a` and `an` are not counts. Watch the
  daemon's "a number in the gist is not in the mail" warning rate before
  loosening anything.
- **A comma or point between digits has more than one reading.** "1.234" is
  verified by 1234 or 1.234 in the mail, so a model that writes "1.234" for
  "1,234" passes. "1,234" is read as thousands only.
- **The peek preview can lag a new message.** It reads the thread through
  the reader's `["thread", id]` query (60 s stale time). A message that
  arrives while the cached thread is fresh has no body there yet, so the
  preview shows nothing until the query refetches.
- **A folded line's to-do is for its latest email.** `t` makes the to-do from
  the newest message in the line. Letting go, tuning and sweeping act on the
  source, so they are unchanged, but an older failed-payment thread in the
  fold has no to-do of its own from this line.
- **Gist requests add a classification read.** `GetThreadGists` now reads
  the sender kind for every conversation in a request (the prompt style is
  part of the cache key), not only the ones with no cached gist. One
  `AccountKinds::load` per account per request.
