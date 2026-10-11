# S04 v0.6.77 installed acceptance

Release `v0.6.77` was built from source `14de28f1187d150e05a9f96f6eec12db60c7c9ab`, tree `48e865128b372f567ec220a682782b5dadeafdea`. [Release workflow 38172695106](https://github.com/planetaryescape/mxr/actions/runs/38172695106) completed successfully. GitHub published the assets at 20:55:52 UTC on 11 October 2026; the workflow completed at 20:58:23 UTC. The mxr binary extracted from the macOS archive matched the installer and Homebrew binaries at `sha256:6489d22c430a3d42b3aadaf64f347e90a6abf6601f25cfaee3bcb1d37a042e6a`.

| Installed surface | Result |
| --- | --- |
| CLI, daemon IPC and HTTP bridge | 17 scenarios passed against the published binary, including lost acknowledgement, restart/query, same-identity replay, changed-payload refusal, and provider uncertainty. [Results](cli.json) |
| MCP | 3 scenarios passed against the published binary, including restart/query and no-repeat handling for an unknown outcome. [Results](mcp.json) |
| TUI | Previewed, explicitly submitted, closed before completion, reopened, and queried the same retained identity. The provider witness records one dispatch and one acceptance; preview had no provider witness. [Results](tui.json), [visible frames](tui-visible-frames.txt) |
| Browser | Confirmed Archive, used Undo, queried the receipt, reloaded, and queried the same identity. The provider witness records one effect, with none before consent. [Journey](browser.json), [after Undo](browser-after-undo.json), [after reload](browser-after-reload.json), [recovery after reload](browser-recovery-after-reload.json) |
| Install artifacts | The mxr binary extracted from the macOS archive matched the installer and Homebrew binaries. [Release verification](release.json), [Homebrew verification](homebrew.json) |

The TUI and browser used synthetic mail. The release workflow skipped its live Gmail smoke because test credentials were unavailable. Linux artifacts were checksum-verified; installed UI journeys ran on macOS. Process-crash checks do not establish recovery after an operating-system crash or power loss. The TUI received its acceptance acknowledgement; dropped-ack recovery was exercised by the CLI/IPC harness. In the browser, waiting for hidden Completed text timed out; opening the recovery panel exposed Query, and Undo plus the subsequent query passed.

The [cleanup record](cleanup.json) reports the browser closed, the owned daemon stopped and the synthetic token removed. Root separately verified ports 18415, 18416 and 18446 were closed.

## Elapsed time and review

| Measure | Observed duration or time |
| --- | --- |
| S04 start to installed acceptance packet observation | 16:13:33.628 to 21:03:33.321 UTC; 4h49m59.693s elapsed |
| Final feature CI, run 38171005526 | 15m32s |
| Release PR CI, run 38172214337 | 5m25s |
| Release workflow, run 38172695106 | 20:33:16 to 20:58:23 UTC; 25m07s |
| Completed independent reviews | 4; 1h08m48s total verifier runtime lower bound. Two attempts were cancelled and one failed because of insufficient Anthropic credit. |

Root and worker active time was not instrumented, so completed verifier runtime is only a lower bound on agent time. One review-waiver policy decision took two brief replies; user-attention duration was not measured. Phases overlapped, so their durations should not be added. The largest measured category was completed independent review, at 1h08m48s. Implementation and integration time were not separately instrumented. S05 is split into client-side download/open followed by immutable upload bound to a saved draft. BK waived independent reviews for the remaining readiness sequence until Anthropic is available again; other gates remain in force.

The machine-readable [manifest](manifest.json) records hashes for the acceptance evidence files, plus source, tree, version and binary digest. The [S04 build log](../../s04-archive-recovery.md) has implementation and check details.
