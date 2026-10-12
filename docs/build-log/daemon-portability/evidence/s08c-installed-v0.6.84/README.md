# S08c published v0.6.84 acceptance

The exact-source release `v0.6.84` was built from `98bc13f574da5cfc93fec1dfc1dddab4d6e996b9` (tree `b4814c3c3631a1d83ee85369b47a81760843d132`). Workflow [38192614021](https://github.com/planetaryescape/mxr/actions/runs/38192614021) succeeded. All four GitHub asset digests matched downloaded bytes, both archive checksum files matched, and the installer and Homebrew binaries had the same mxr SHA-256: `ee9d794e9411f5d754f6ede069feeb660d351fb2903990958a54214aedeef7c8`.

Published supervised acceptance passed on macOS in 8.590 seconds and Linux/amd64 in 14.478 seconds. Each run persisted four synthetic search results and recorded three controlled model requests. The Linux test used a network-disabled Docker container with two CPUs and 2 GiB memory; systemd was not installed, so this is evidence for the published binary under the supervised fixture, not an installed systemd service.

The installed binary also passed native TUI acceptance in a 150×36 PTY. Search against the compatible daemon returned one result. On reconnect to a frozen v0.6.82 daemon replacement on the same socket, v0.6.84 rejected the peer, displayed service-manager guidance and left the replacement alive and unchanged until owned cleanup. The TUI exited with status 0, and its owned processes and socket were removed. The captured frames show the search and reconnect journey; only redundant repeated status renders were omitted from the interactive tool output. Long reconnect guidance remains clipped, and the existing retry watchdog issue remains tracked in [the repository issue](../../../../issues/tui-reconnect-watchdog-guidance.md).

The production headless guide is deployed from source `98bc13f5` at [mxr.sh/guides/headless-linux](https://mxr.sh/guides/headless-linux/). The browser check found no JavaScript errors and rendered command tokens matched the source. The compact deployment receipt and screenshot are retained here.

This packet keeps compact acceptance summaries and provenance hashes, not release metadata with creator details, full logs or binaries. The [candidate packet](../s08c-candidate-4b2d19d6/README.md) remains the record of root review, CI, merge gates and pre-release fixture evidence. The final S08c documentation closeout PR has not yet been deployed.

Known limits include the Linux container's lack of an installed systemd service, process-local analytics queue recovery, SMTP failure aggregation in doctor, and the TUI reconnect watchdog/guidance display issue. The analytics and SMTP items are not established by these acceptance runs.
