# A crafted spreadsheet attachment can crash the daemon until calamine 0.38 is allowed

Status: open, found 2026-11-04. Revisit on 2026-11-09.

RUSTSEC-2026-0436: calamine 0.37's CFB/OLE2 parser (`src/cfb.rs`) panics on a directory stream whose length is not a multiple of 128 and allocates without bound on a self-referential DIFAT chain. A 512-byte file can ask for several gigabytes.

mxr reaches it. Release builds enable `semantic-local`, and `read_spreadsheet_tables` in `crates/semantic/src/lib.rs` opens downloaded spreadsheet attachments with `open_workbook_auto`. The `.xls` and `.xlsb` readers parse CFB directly, and the `.xlsx` reader runs `Cfb::new` on the file to check for password protection, so no spreadsheet format avoids it. Parsing runs inside semantic chunk preparation (`build_chunks`, called from `load_or_prepare_chunks`), so a crafted attachment gives a panic in the semantic indexing path or an out-of-memory abort of the daemon. Neither is memory unsafety.

The fix is calamine 0.38.0, published to crates.io on 2026-11-02T01:32Z. Our rule keeps crates out until they are seven days old, so `deny.toml` ignores the advisory until then.

## Next action

On or after 2026-11-09, set `calamine = { version = "0.38", optional = true }` in `crates/semantic/Cargo.toml`, run `cargo update -p calamine`, delete the RUSTSEC-2026-0436 entry from `deny.toml`, run `scripts/cargo-test -p mxr-semantic --features local --tests` and `cargo deny check advisories`, and close this issue.
