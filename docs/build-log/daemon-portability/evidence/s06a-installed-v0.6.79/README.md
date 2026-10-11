# Published v0.6.79 preserves draft text through an event gap

Source `f56ac1527787ca312bda7b52935947ee94495251`, tree `6363d8b3058b6924827b335ff47d311a4aa15c2f`. [Release workflow](https://github.com/planetaryescape/mxr/actions/runs/38179507055) passed, and [v0.6.79](https://github.com/planetaryescape/mxr/releases/tag/v0.6.79) published at 22:35:20 UTC on 11 October 2026. Both platform archives matched their published checksums. The macOS archive, isolated installer and Homebrew binary matched SHA-256 `9999eb3ce4d3a84785ef8ecaf9f7aa5e41f014f549284d744aade7876b8a376e`; the only distinct mxr path was `/opt/homebrew/bin/mxr`.

The actual embedded browser passed at 22:38:28 UTC. HTTP stayed healthy while a loopback proxy blocked WebSockets. After a CLI Archive preview and confirmed operation, reconnecting refreshed the mailbox and removed the archived row. Compose retained text whose save had deliberately failed before the gap. The server draft did not contain that text. No compose refetch or effectful mutation replay occurred; the browser error and console lists were empty. [Acceptance](acceptance.json), [network events](gap-events.json), [before](before-gap.json), [after](after-gap.json), [screenshot](recovered.png).

POST arrivals/modes performs the existing read-only GetArrivalModes lookup and is allowed by the replay assertion. Synthetic mail and loopback traffic establish this reconnect behavior, not live-provider or off-machine HTTP behavior. The Linux archive was checksum-verified; this browser journey ran on macOS.

The owned browser, daemon and proxy closed; ports18458/18459 were closed. No personal daemon restart occurred. [Cleanup](cleanup.json), [release verification](release.json), [Homebrew](homebrew.json), [file hashes](manifest.json).

| Measured phase | Elapsed |
| --- | --- |
| Combined candidate build | 1m19s, plus19.5s queue |
| Feature CI38177782786 | 17m05s |
| Release workflow38179507055 | 24m51s |
| Publication to browser acceptance | 3m08s |

The release build was the longest measured phase. Worker/root active time was not separately instrumented. Phases overlapped; do not add these durations. Root review required no independent-review wait under BK's temporary Anthropic waiver. [Build log](../../s06a-event-gap-recovery.md).
