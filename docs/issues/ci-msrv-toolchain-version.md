# The MSRV job requests an unavailable Rust version

Status: open. Observed 11 October 2026 on main `f1e19fba2055fa96bbaf73ce35a6bd8354cd4870`; unchanged in the Categories candidate `448e1da551b2570e23c20801bab4333ee8ef5381`.

`.github/workflows/ci.yml:450` names the job `MSRV (1.94)`, matching `Cargo.toml:172`, but its toolchain action at line 456 requests `dtolnay/rust-toolchain@1.120`. The [completed CI job](https://github.com/planetaryescape/mxr/actions/runs/38148073022/job/114495570202) failed before compilation: rustup requested `1.120.0` and received HTTP 404.

The job runs when the workflow's `expensive` output is true, including manual dispatch. Ordinary PR runs skip it, so passing PR checks do not establish the advertised minimum compiler version.

Align the toolchain action with the supported minimum, then run the existing locked workspace check with that compiler. Confirm dependency compatibility before claiming Rust 1.94 support. Repeating the unchanged download cannot resolve this failure.
