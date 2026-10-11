---
title: Run mxr under systemd on Linux
description: Run the foreground daemon with systemd as its sole process owner.
---

Run the daemon in the foreground under a user service. The service manager owns
startup and replacement; clients use the same profile paths and set
`MXR_DAEMON_AUTOSTART=off` so an outage never creates a second daemon.

Create `~/.config/systemd/user/mxr.service`:

```ini
[Unit]
Description=mxr mail daemon
[Service]
Type=simple
Environment=HOME=%h
Environment=XDG_RUNTIME_DIR=%t
Environment=MXR_CONFIG_DIR=%h/.config/mxr
Environment=MXR_DATA_DIR=%h/.local/share/mxr
Environment=MXR_DAEMON_AUTOSTART=off
Environment=MXR_KEYCHAIN=off
ExecStart=%h/.local/bin/mxr daemon --foreground
Restart=on-failure
RestartSec=3
TimeoutStopSec=45
KillSignal=SIGTERM

[Install]
WantedBy=default.target
```

Enable it with `systemctl --user daemon-reload` and
`systemctl --user enable --now mxr.service`. For services that should start
before login, enable lingering with `loginctl enable-linger <user>`.

Run clients with matching identity settings:

```sh
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export MXR_CONFIG_DIR="$HOME/.config/mxr"
export MXR_DATA_DIR="$HOME/.local/share/mxr"
export MXR_DAEMON_AUTOSTART=off
export MXR_KEYCHAIN=off
mxr status --format json
```

The service and clients must resolve to the same config, data and runtime
directories. With autostart off, a reachable compatible daemon remains usable.
If it is unavailable or built for an incompatible protocol, mxr tells you to
inspect or restart the systemd service; it does not spawn or replace the
process.

With `MXR_KEYCHAIN=off`, IMAP/SMTP passwords must be in the active config
directory's `secrets.toml` file, protected with mode `0600`. Gmail and Outlook
OAuth keep their existing token-storage behavior. After a sync attempt, run
`mxr doctor --format json`; an observed missing IMAP sync credential identifies
the account and gives disk-secret or keychain remediation without printing a
secret or endpoint URL. Doctor does not aggregate SMTP send failures yet.

LLM providers are optional. A model request failure appears in the daemon's
process-local LLM status and in the affected feature's health; a later successful
request clears that feature's degraded state. Status does not probe the model,
and core mail readiness does not depend on model availability.

The daemon handles SIGTERM through orderly shutdown. Keep the service stop
deadline finite because already-running blocking work cannot always be
cancelled. A stopped analytics backlog may need a separate rebuild; this recipe
does not promise automatic replay.
