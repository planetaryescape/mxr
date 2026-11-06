---
title: Keep a private mailbox host running
description: Preview and install an owned Tailscale HTTPS service, inspect connected clients, and stop hosting or the whole daemon.
---

Configure your mail account on the host and connect the host to Tailscale before installing the service. Use a computer that can stay awake and online. Keep clients and the host on the same mxr version.

## Preview the exact service and HTTPS port

```sh
mxr serve setup --public-origin https://mail-host.example.ts.net:8443 --dry-run
```

Replace `mail-host.example.ts.net` with this host's DNS name from `tailscale status --json`. The origin must use HTTPS and this node's Tailscale DNS name. The gateway listens at `127.0.0.1:18741` unless you pass `--listen`.

The preview reports the profile paths, workspace identity, service file, startup behavior and intended HTTPS mapping. It checks the local daemon, recovery state, Tailscale login, existing Serve mappings and user supervisor. It creates neither a service nor a proxy. If another service uses the selected HTTPS port, choose a different port; mxr refuses to replace its mapping.

On Linux, unattended startup requires a working user systemd manager and user lingering. If the preview reports missing lingering, run the administrator command it prints and repeat the preview. On macOS, the LaunchAgent starts at user login; it does not provide service before login. mxr does not run the daemon as root or change tailnet policies.

Apply the previewed setup:

```sh
mxr serve setup --public-origin https://mail-host.example.ts.net:8443 --confirm
mxr serve status --format json
```

Setup records its progress before changing the service or proxy. It waits for the authenticated gateway before publishing the HTTPS mapping, then checks HTTPS health and rejection of anonymous IPC. Pair an independent client using the [remote mailbox guide](/guides/remote-mailbox/) to verify approved access. Setup checks do not substitute for that pairing and mail workflow.

## Serve a custom HTTPS origin behind your own proxy

`serve setup` publishes through Tailscale Serve and requires the node's
Tailscale DNS name. If the mailbox answers on a domain you already terminate
with Caddy, nginx or another reverse proxy, run the gateway in the foreground
and supervise it with your own service unit:

```sh
mxr serve --foreground \
  --listen 127.0.0.1:18741 \
  --public-origin https://mail.example.com
```

Foreground mode writes no `hosting.json` and installs no supervisor. Do not
also run `serve setup` for the same host; it owns a Tailscale Serve mapping and
would refuse a non-tailnet origin.

- Proxy the whole origin to `127.0.0.1:18741` and preserve the incoming `Host`
  header exactly. The gateway rejects any other host, and `X-Forwarded-*`
  headers never establish browser trust.
- Native requests must not send `Origin`. Browser POSTs and WebSocket upgrades
  must send exactly the configured origin.
- Release binaries built with `web-ui` embed the SPA, so one proxy can serve
  `/` and `/api/*`. If you serve a separately built SPA from a static root,
  proxy `/api/*` and leave `/` on that root.
- The session cookie is `__Host-mxr-session`: `Secure`, `HttpOnly`,
  `SameSite=Strict`, `Path=/`, and no `Domain`. It is host-only to the
  configured origin and lasts at most thirty days. The proxy must not rewrite
  those attributes, and the origin needs a valid certificate.

## Inspect registrations and current sessions separately

```sh
mxr serve clients --format json
mxr serve status --format json
```

Registrations persist until expiry or revocation. Presence is a per-boot lease: the status includes each session's surface, last-seen time, lease expiry and active flag. A closed tab or expired lease does not deregister its installation. A new daemon boot clears presence and gives clients a new boot ID; clients must authenticate and refresh their mailbox state again.

To remove a registration, preview its exact ID before revoking it:

```sh
mxr serve clients revoke <client-id> --dry-run
mxr serve clients revoke <client-id> --confirm
```

## Stop remote access while local sync continues

```sh
mxr serve stop --dry-run
mxr serve stop --confirm
```

The preview reports registered clients, leased sessions, accepted remote operations and scheduled sends. Execution persists stopped hosting intent, closes admission, emits a stopping notice and gives accepted remote requests a bounded drain window. It stops the owned user service and removes only its recorded HTTPS mapping. The local daemon can continue sync and scheduled work.

Resume recorded hosting explicitly:

```sh
mxr serve start --dry-run
mxr serve start --confirm
```

## Stop the provider owner before moving its workspace

```sh
mxr serve stop --all --dry-run
mxr serve stop --all --confirm
```

`mxr daemon stop --confirm` also stops the full hosted profile. A full stop persists disabled daemon intent. Client commands cannot reverse it through autostart; use `mxr serve start --confirm` to resume hosting, or `mxr daemon` to start only the local owner.

Follow the [backup and recovery guide](/guides/backup-recovery/) after stopping the source. Restore into an empty destination profile, provision credentials there, inspect its paused recovery state, confirm that the source owner remains stopped, and activate the destination explicitly. Re-pair clients; portable backups do not carry remote credentials. Hosting setup does not copy a mailbox or transfer ownership.

## Retry incomplete setup without replacing other services

```sh
mxr serve status --format json
tailscale serve status --json
```

A failed setup retains its reconciliation record at the profile's `hosting.json`. Repeat the same setup command after fixing the reported prerequisite. If a service file, HTTPS target, workspace or recovery generation changed, inspect that change before retrying. The supervisor checks recorded workspace identity before starting provider work and refuses a replaced or paused restored workspace.

If cleanup fails during stop, hosting intent remains disabled. The command attempts service stop before removing the mapping and reports any remaining cleanup. Check the reported service and proxy state before treating either as stopped. mxr never calls `tailscale serve reset` and never enables Funnel.
