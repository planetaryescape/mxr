# chrono and chrono-tz are unmaintained

Status: open

RustSec marks `chrono` (RUSTSEC-2026-0543) and `chrono-tz` (RUSTSEC-2026-0555) as unmaintained. Neither advisory reports a vulnerability, and neither crate has a safe upgrade.

`deny.toml` ignores both advisories so that `cargo deny` still fails on new problems. 30 crate manifests reference `chrono`, so the replacement needs its own change.

## Done when

- The workspace uses a maintained date and time library.
- Stored timestamps, date parsing and time zone display keep their current behaviour.
- `deny.toml` no longer ignores these two advisories.
