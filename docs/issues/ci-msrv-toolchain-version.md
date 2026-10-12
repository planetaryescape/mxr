# The MSRV job requests an unavailable Rust version

Status: resolved by [PR #401](https://github.com/planetaryescape/mxr/pull/401), merged as `55ff4fb8eb5fb93911f098b057c7bc87552e53a6`. Observed 11 October 2026 on main `f1e19fba2055fa96bbaf73ce35a6bd8354cd4870`.

At that revision, `.github/workflows/ci.yml:450` named the job `MSRV (1.94)`, matching `Cargo.toml:172`, but its toolchain action at line 456 requested `dtolnay/rust-toolchain@1.120`. The [completed CI job](https://github.com/planetaryescape/mxr/actions/runs/38148073022/job/114495570202) failed before compilation: rustup requested `1.120.0` and received HTTP 404.

The job runs when the workflow's `expensive` output is true, including manual dispatch. Ordinary PR runs skip it, so passing PR checks do not establish the advertised minimum compiler version.

The workflow now reads `workspace.package.rust-version` from `Cargo.toml` and uses that value for both the job name and installed toolchain. The [Rust 1.94 locked workspace check](https://github.com/planetaryescape/mxr/actions/runs/38184717102/job/114604526296) passed on candidate `669317d2ecef390497a7d47c765cf7bada013c29`; its tree is the merged tree above.
