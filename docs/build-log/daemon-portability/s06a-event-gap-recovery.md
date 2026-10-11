# Mail refreshes after the event connection returns

S06a merged in [PR391](https://github.com/planetaryescape/mxr/pull/391) at
`d9ceba71a8aa17de9acc0b50d3067e123bcfdb85`, tree
`489f027143c4ea42fa9c5d8f8502279a56d93cb1`. Published v0.6.79 passed installed-client acceptance. [PR393](https://github.com/planetaryescape/mxr/pull/393)
passed CI and merged at `f56ac1527787ca312bda7b52935947ee94495251`, tree
`6363d8b3058b6924827b335ff47d311a4aa15c2f`. [Tag workflow 38179507055](https://github.com/planetaryescape/mxr/actions/runs/38179507055)
completed successfully at 22:37:34 UTC on 11 October 2026.

A dropped WebSocket could leave the inbox stale while HTTP requests continued
to succeed. Published v0.6.77 reproduced this: archive a synthetic message through
the CLI during an event-only outage, restore the socket, and the browser still
shows the message. There was no mailbox request after reconnection.

The availability listener now remembers a gap after an established connection
drops and invalidates queries when it reconnects. Initial connections and
repeated connected notifications do not trigger another refresh. HTTP recovery
does not close the gap before the event connection returns. The existing
`keepThroughGaps` opt-out preserves compose state.

The changed path is
[`daemonAvailability.ts:122`](https://github.com/planetaryescape/mxr/blob/d9ceba71a8aa17de9acc0b50d3067e123bcfdb85/apps/web/src/lib/daemonAvailability.ts#L122).
There is no protocol or persisted-data change. Workspace and recovery-generation
transitions remain S06b.

## The browser check preserves text that was never saved

The local candidate at `9cb9c7ef76d5e8516e3759a1c469c7fbbc4ef042` passed this
journey in the embedded web app:

1. Open the synthetic inbox and compose a reply. Reject its save request with
   a controlled ordinary HTTP error, then enter text and observe the unsaved
   status. Confirm the text is absent from the daemon's draft file.
2. Block only WebSocket traffic. Keep HTTP status requests working and archive
   a message through CLI preview and confirmed execution.
3. Restore WebSocket traffic. Observe a mailbox fetch, the archived row removed,
   and the unsaved compose text still present.
4. Confirm no compose reload or effectful mutation replay, no JavaScript errors,
   and shutdown of the owned browser, proxy and daemon.

The replay check allows the existing POST `arrivals/modes`: its handler performs
the read-only `GetArrivalModes` lookup. HTTP method alone does not establish an
effect. The runner uses fake mail, a loopback proxy and an isolated profile;
it does not test a live provider or off-machine HTTP.

## Verification follows the merged tree

The worker ran 14 focused availability/event invalidation tests plus web
typecheck, lint and format checks. Root reviewed the source in a separate clean
clone, built the SPA and `cargo build -p mxr --features web-ui`, and repeated the
browser journey after integrating current main. The combined binary build took
1m19s after 19.5s in the shared build queue.

CI [38177782786](https://github.com/planetaryescape/mxr/actions/runs/38177782786)
passed on the exact reviewed head. Its browser suite reported 383 passes and
two cases that passed on retry: the controls inventory and natural-time snooze
tests. The production speed gate passed all four cases. The full CI run took
17m05s; the serial browser suite took 13m31s. The two preceding successful
browser jobs took 15m23s and 15m08s including setup, so the full suite remains
the main wait. No concurrency change or time saving is claimed.

An initial local build omitted the optional `web-ui` feature and served no
embedded app. Rebuilding with that feature resolved the setup failure. The
corrected journey ran against the resulting binary, not a development server.

Routing remained Fast GPT-6-Luna/high for the worker, with root review and no
escalation. BK waived independent reviews until Anthropic is available again;
all other gates remain. No additional user attention was required for S06a.

The [published acceptance packet](evidence/s06a-installed-v0.6.79/README.md) binds the release, installer, Homebrew and actual browser journey to the same binary. Both archive checksums matched. The installed browser repeated the event-only outage with dirty compose text preserved, fresh mailbox state and no effectful replay. Its error and console lists were empty. The owned fixture closed. The release workflow took 24m51s; publication-to-browser acceptance took 3m08s. Documentation publication is the remaining closeout step.
