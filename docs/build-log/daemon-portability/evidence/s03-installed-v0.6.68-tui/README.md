# Published v0.6.68 TUI acceptance

The installed Homebrew binary at v0.6.68 passed provider-save conflict, explicit retry, same-file in-flight ownership and retained-Unknown restart/query journeys. The run took 385.907 seconds and used only the fake provider. The binary SHA-256 was `51a89aec14f4b9c3fae384c567f825e2fd575239f9a34369ee1cfb5259c860ee`; source was `621e9e49739eeb80abbccea0916578420941c3fc`.

`witness-summary.json` contains only the outcome fields needed for the closeout. `visible-frames.txt` keeps the four relevant user-facing messages. Original source hashes and the selection boundary are in `selection-provenance.json`.

The canonical revision-2 save and fake-provider link are observed in SQLite; provider memory was not inspected. The initial stale save preserved the compose scratch until explicit discard. The earlier `provider-saved.json` snapshot is a revision-1 rejection/no-op and is deliberately excluded. In the other Unknown profile, scratch disappeared only after the user explicitly discarded it. The retained-Unknown profile is separate and proves scratch and operation identity survived the restart, query and blocked send. Frames are pyte reconstructions, not screenshots. An initial malformed helper request caused an IPC decode error before the corrected competing update; it produced no provider effect.
