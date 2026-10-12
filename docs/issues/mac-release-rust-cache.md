# Mac release builds do not restore the Rust target cache

The tagged macOS builds for v0.6.64 and v0.6.65 reported no Rust target-cache restore and zero Rust `sccache` hits. Their native build steps took 19m17s and 19m22s. The logs recorded 683 and 620 `sccache` write errors, without stating a cause. The [curated profile](../build-log/daemon-portability/evidence/release-build-cache-profile-v0.6.64-v0.6.65.json) preserves the run IDs, source SHAs, cache keys and raw-log hashes. The measured compile-time savings are unknown.

GitHub documents that a workflow triggered by one tag cannot restore a cache created for another tag name: [cache access restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache). Investigate whether a compatible target cache can be restored from the trusted default branch, and inspect cache-write diagnostics in a future run. Verify cache restoration and Rust hits before measuring savings. This is an investigation only; no workflow or source change is included here.


The v0.6.80 macOS job repeated the cache miss: no Rust target cache restored,
zero Rust `sccache` hits,770 Rust misses and573 cache-write errors. Its native
build step took25m27s and succeeded. The [v0.6.80 profile](../build-log/daemon-portability/evidence/release-build-cache-profile-v0.6.80.json)
records the run, source and raw-log hash. This confirms the missed cache benefit
continues; it does not establish why cache writes failed or how much time a fix
would save.
