# Command-bridge failures need target-aware guidance

Status: deferred to portability S07. Source: `5de44f93a8ae3cf8ff90d6a53738ea98bf2366db`, independent S01 review. [Readiness blueprint](../blueprint/23-daemon-portability.md) owns the remote-client scope.

A command transport connects when its child process starts (`crates/client/src/lib.rs:175-195`). If SSH or `dial-stdio` then exits, the first request reports connection-closed/protocol or broken-pipe guidance (`crates/daemon/src/ipc_client.rs:167-195`). The command arguments remain redacted, but the message does not explain how to check the selected bridge. Verify the actual error with a synthetic child that exits before responding, then give target-aware recovery advice without implying that restarting the local default fixes the remote endpoint.

Keep explicit `MXR_DAEMON_ADDR` selections isolated from local lifecycle, even when a Unix path happens to match the local default. S01 uses that boundary deliberately. Broader command targeting belongs in S07.

Also verify daemon-generated doctor suggestions at `crates/daemon/src/status_helpers.rs:342,420,444`: they use `mxr daemon --restart`, while the CLI exposes a separate restart command. Correct the owning suggestions and tests in an appropriate diagnostics slice; remote table output already qualifies which host owns them.

No runtime reproduction was performed for these follow-ups. S01 source and real-daemon smoke show no local fallback; the independent reviewer classified these as nonblocking guidance defects. Exact next action: reproduce the child-exit path while implementing S07, then update its target-aware error and command guidance together.
