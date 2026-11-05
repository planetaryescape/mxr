# IMAP ignores `use_tls = false`

**Status:** open · **Severity:** medium

The IMAP session always connects over TLS. `ImapConfig::use_tls`
(`crates/provider-imap/src/config.rs`) is parsed and stored, but nothing in
`crates/provider-imap/src/session.rs` reads it: both the sync session and the
IDLE watcher wrap the TCP stream in `async_native_tls` unconditionally. An
account set to a plain IMAP port with `use_tls = false` fails every sync with
`Connection error: record overflow` (the TLS client reading a plaintext
greeting), and the error doesn't say why.

Found on 2026-11-04 while smoke-testing the newest-first first sync against
GreenMail's plain IMAP port (3143).

Who hits it: servers that only offer STARTTLS on port 143, and local bridges
that listen on loopback without implicit TLS.

Open decision before fixing: plaintext IMAP sends the password unencrypted.
Options are to honour `use_tls = false` only for loopback hosts, add STARTTLS
as the default for port 143, and make the error name the TLS mismatch. Each
needs a decision on certificate trust for self-signed local bridges.
