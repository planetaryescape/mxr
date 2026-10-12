# Remote browser bootstrap is blocked by the SPA connect-src policy

The current remote-browser bootstrap accepts a selected bridge origin and
token, stores them in browser storage, and removes them from the location hash.
The served SPA's Content Security Policy still sets
`connect-src 'self' ws: wss:`, so the browser refuses the cross-origin health
fetch before it reaches the selected bridge. A separate CORS preflight to that
bridge returned 200, but both normal and `no-cors` health fetches failed with
`TypeError: Failed to fetch`; the same-origin health request returned 200.

The source is unchanged between the current main ref `cab8c4c65feef2489fecaa0172783f7476ee0d90` and S06b candidate `bb64d37d88c0372a6caf0d4dcd3a5e08e3af787b`: `crates/web/src/spa.rs` emits the self-only `connect-src` policy. The browser observation used frozen binary source `0b0591884de84695d5cd494433a19e9489716a16` (binary SHA-256 `8be64bceb411e97ebf16cfc373c40064d8a36d5477ad97b90eb66572761a4e8d`) and is retained in the [candidate packet](../build-log/daemon-portability/evidence/s06b-candidate-bb64d37/remote-browser-csp.json).

No cross-origin bridge health fetch succeeded in this check. Remote HTTP access remains deferred under R01; this finding does not authorize weakening CSP. The S06b same-origin workspace-replacement journey is separate and remains part of the stage acceptance. Resolve remote access through the planned remote-access security design, then add a browser test that verifies the selected target is reached without exposing the bridge token.
